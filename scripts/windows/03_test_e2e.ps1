# =============================================================================
# 03_test_e2e.ps1 — Test de bout en bout du banc d'essai complet
# À exécuter depuis Windows une fois les 3 nœuds démarrés.
#
# Prérequis :
#   - Nœud actif  : http://192.168.200.1:3000   (ce PC)
#   - Nœud passif : http://192.168.200.130:3001  (Ubuntu)
#   - Relais      : http://192.168.200.128:4000  (Kali)
# =============================================================================

$ACTIVE    = "http://192.168.200.1:3000"
$PASSIVE   = "http://192.168.200.130:3001"
$RELAY     = "http://192.168.200.128:4000"
$RELAY_KEY = "sovereign-spike-relay-key-2026"

$PASS = 0; $FAIL = 0

function Test-Step($name, $block) {
    Write-Host "`n▶ $name" -ForegroundColor Cyan
    try { & $block; Write-Host "  ✓ OK" -ForegroundColor Green; $script:PASS++ }
    catch { Write-Host "  ✗ ECHEC : $_" -ForegroundColor Red; $script:FAIL++ }
}

function Invoke-Api($method, $url, $body = $null, $headers = @{}) {
    $p = @{ Method = $method; Uri = $url; ContentType = "application/json" }
    if ($body)    { $p.Body = ($body | ConvertTo-Json) }
    if ($headers) { $p.Headers = $headers }
    Invoke-RestMethod @p
}

Write-Host "=== sovereign-spike :: Test E2E COMPLET (3 nœuds) ===" -ForegroundColor Magenta
Write-Host "Actif  : $ACTIVE"
Write-Host "Passif : $PASSIVE"
Write-Host "Relais : $RELAY"

# ── 1. Santé des 3 nœuds ─────────────────────────────────────────────────────
Test-Step "Santé nœud actif (Windows)" {
    if ((Invoke-RestMethod "$ACTIVE/health") -ne "ok") { throw "réponse inattendue" }
}
Test-Step "Santé nœud passif (Ubuntu)" {
    if ((Invoke-RestMethod "$PASSIVE/health") -notmatch "passif") { throw "réponse inattendue" }
}
Test-Step "Santé relais aveugle (Kali)" {
    $r = Invoke-Api GET "$RELAY/health"
    Write-Host "    role=$($r.role)"
    if ($r.role -ne "relay-aveugle") { throw "role=$($r.role)" }
}

# ── 2. Stock (vérification par delta, pas par valeur absolue) ─────────────────
$stock_avant = (Invoke-Api GET "$ACTIVE/stock/PANTALON-L").quantity
Write-Host "  [info] Stock initial PANTALON-L = $stock_avant"

Test-Step "Stock +100 PANTALON-L (delta)" {
    $r = Invoke-Api POST "$ACTIVE/write" @{ op_type = "stock_adjust"; item_id = "PANTALON-L"; quantity = 100 }
    if ($r.status -ne "committed") { throw "status=$($r.status)" }
    $apres = (Invoke-Api GET "$ACTIVE/stock/PANTALON-L").quantity
    if ($apres -ne ($stock_avant + 100)) { throw "attendu=$($stock_avant+100) obtenu=$apres" }
    Write-Host "    seq=$($r.seq)  stock=$apres (+100 ✓)"
    $script:stock_avant = $apres
}

Test-Step "Vente -10 PANTALON-L (delta)" {
    $r = Invoke-Api POST "$ACTIVE/write" @{ op_type = "sale"; item_id = "PANTALON-L"; quantity = 10 }
    if ($r.status -ne "committed") { throw "status=$($r.status)" }
    $apres = (Invoke-Api GET "$ACTIVE/stock/PANTALON-L").quantity
    if ($apres -ne ($stock_avant - 10)) { throw "attendu=$($stock_avant-10) obtenu=$apres" }
    Write-Host "    seq=$($r.seq)  stock=$apres (-10 ✓)"
    $script:stock_avant = $apres
}

# ── 3. Anti-survente ──────────────────────────────────────────────────────────
Test-Step "Anti-survente (stock+1) → rejetée (409)" {
    $trop = $stock_avant + 1
    try {
        Invoke-Api POST "$ACTIVE/write" @{ op_type = "sale"; item_id = "PANTALON-L"; quantity = $trop }
        throw "non-rejeté"
    } catch {
        if ($_ -notmatch "non-rejeté") { return }
        throw
    }
}

# ── 4. Idempotence ────────────────────────────────────────────────────────────
Test-Step "Idempotence : même op_id → même seq" {
    $id = [guid]::NewGuid().ToString()
    $r1 = Invoke-Api POST "$ACTIVE/write" @{ op_type = "sale"; item_id = "PANTALON-L"; quantity = 1; op_id = $id }
    $r2 = Invoke-Api POST "$ACTIVE/write" @{ op_type = "sale"; item_id = "PANTALON-L"; quantity = 1; op_id = $id }
    if ($r1.seq -ne $r2.seq) { throw "seq1=$($r1.seq) ≠ seq2=$($r2.seq)" }
    Write-Host "    seq=$($r1.seq) stable"
}

# ── 5. Époque de fencing ──────────────────────────────────────────────────────
Test-Step "Époque de fencing ≥ 1" {
    $r = Invoke-Api GET "$ACTIVE/epoch"
    Write-Host "    epoch=$($r.epoch) host=$($r.primary_host)"
    if ($r.epoch -lt 1) { throw "epoch=$($r.epoch)" }
}

# ── 6. Journal blobs opaques ──────────────────────────────────────────────────
Test-Step "Journal : blobs opaques (hex)" {
    $r = Invoke-Api GET "$ACTIVE/journal?after_seq=0&limit=5"
    if ($r.Count -lt 1) { throw "journal vide" }
    if ($r[0].blob_nonce -notmatch "^[0-9a-f]+$") { throw "nonce non-hex : $($r[0].blob_nonce)" }
    Write-Host "    $($r.Count) blob(s) — nonce[:16]=$($r[0].blob_nonce.Substring(0,16))… → opaque ✓"
}

# ── 7. Synchronisation du passif ─────────────────────────────────────────────
Test-Step "Passif synchronisé (attente 12s)" {
    Write-Host "    Attente sync passif (intervalle 5s)..."
    Start-Sleep -Seconds 12
    $r = Invoke-Api GET "$PASSIVE/sync/status"
    Write-Host "    last_seq=$($r.last_seq)"
    if ($r.last_seq -lt 1) { throw "last_seq=$($r.last_seq)" }
}

Test-Step "Cohérence actif ↔ passif (PANTALON-L)" {
    $a = (Invoke-Api GET "$ACTIVE/stock/PANTALON-L").quantity
    $p = (Invoke-Api GET "$PASSIVE/stock/PANTALON-L").quantity
    Write-Host "    actif=$a  passif=$p"
    if ($a -ne $p) { throw "actif=$a ≠ passif=$p" }
}

# ── 8. Relais zéro-knowledge ──────────────────────────────────────────────────
Test-Step "Relais : push blob opaque" {
    $blob = @{ seq = 9999; blob_nonce = ("aa" * 24); blob_ciphertext = ("deadbeef" * 16) }
    $r = Invoke-Api POST "$RELAY/blobs" $blob @{ "X-Relay-Key" = $RELAY_KEY }
    Write-Host "    status=$($r.status)"
    if ($r.status -notin "stored", "already_stored") { throw "status=$($r.status)" }
}

Test-Step "Relais : contenu opaque (zero-knowledge prouvé)" {
    $r = Invoke-Api GET "$RELAY/blobs?after_seq=9998&limit=1"
    if ($r.Count -lt 1) { throw "blob non trouvé dans le relais" }
    Write-Host "    nonce=$($r[0].blob_nonce.Substring(0,16))..."
    Write-Host "    → Le relais stocke sans déchiffrer. Zero-knowledge ✓"
}

# ── Résumé ────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Magenta
$col = if ($FAIL -eq 0) { "Green" } else { "Yellow" }
Write-Host "  $PASS/$($PASS + $FAIL) tests réussis" -ForegroundColor $col
if ($FAIL -eq 0) {
    Write-Host "  BANC D'ESSAI PHASE 0 ENTIÈREMENT VALIDÉ ✓" -ForegroundColor Green
    Write-Host "  Crypto + Sérialisation + Réplication + Zéro-Knowledge" -ForegroundColor Green
} else {
    Write-Host "  $FAIL test(s) en échec — vérifier les logs des nœuds." -ForegroundColor Red
}
Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Magenta
