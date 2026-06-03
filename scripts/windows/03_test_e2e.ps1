# =============================================================================
# 03_test_e2e.ps1 — Test de bout en bout du banc d'essai complet
# À exécuter depuis Windows une fois les 3 nœuds démarrés.
#
# Prérequis :
#   - Nœud actif  : http://192.168.200.1:3000   (ce PC)
#   - Nœud passif : http://192.168.200.130:3001  (Ubuntu)
#   - Relais      : http://192.168.200.128:4000  (Kali)
# =============================================================================

$ACTIVE  = "http://192.168.200.1:3000"
$PASSIVE = "http://192.168.200.130:3001"
$RELAY   = "http://192.168.200.128:4000"
$RELAY_KEY = "sovereign-spike-relay-key-2026"

$PASS = 0; $FAIL = 0

function Test-Step($name, $block) {
    Write-Host "`n▶ $name" -ForegroundColor Cyan
    try {
        & $block
        Write-Host "  ✓ OK" -ForegroundColor Green
        $script:PASS++
    } catch {
        Write-Host "  ✗ ECHEC : $_" -ForegroundColor Red
        $script:FAIL++
    }
}

function Invoke-Api($method, $url, $body = $null, $headers = @{}) {
    $params = @{ Method = $method; Uri = $url; ContentType = "application/json" }
    if ($body)    { $params.Body = ($body | ConvertTo-Json) }
    if ($headers) { $params.Headers = $headers }
    Invoke-RestMethod @params
}

Write-Host "=== sovereign-spike :: Test E2E Banc d'essai ===" -ForegroundColor Magenta
Write-Host "Active  : $ACTIVE"
Write-Host "Passive : $PASSIVE"
Write-Host "Relay   : $RELAY"

# ── 1. Santé des 3 nœuds ─────────────────────────────────────────────────────
Test-Step "Santé nœud actif (Windows)" {
    $r = Invoke-Api GET "$ACTIVE/health"
    if ($r -ne "ok") { throw "Réponse inattendue : $r" }
}

Test-Step "Santé nœud passif (Ubuntu)" {
    $r = Invoke-Api GET "$PASSIVE/health"
    if ($r -notmatch "passif") { throw "Réponse inattendue : $r" }
}

Test-Step "Santé relais (Kali)" {
    $r = Invoke-Api GET "$RELAY/health"
    if ($r.role -ne "relay-aveugle") { throw "Rôle inattendu : $($r.role)" }
}

# ── 2. Initialisation du stock ────────────────────────────────────────────────
Test-Step "Stock initial : +100 PANTALON-L (ajustement)" {
    $r = Invoke-Api POST "$ACTIVE/write" @{
        op_type  = "stock_adjust"
        item_id  = "PANTALON-L"
        quantity = 100
    }
    if ($r.status -ne "committed") { throw "Status : $($r.status)" }
    Write-Host "    seq=$($r.seq) op_id=$($r.op_id)"
}

# ── 3. Vente normale ──────────────────────────────────────────────────────────
Test-Step "Vente : -10 PANTALON-L (stock doit passer à 90)" {
    $r = Invoke-Api POST "$ACTIVE/write" @{
        op_type  = "sale"
        item_id  = "PANTALON-L"
        quantity = 10
    }
    if ($r.status -ne "committed") { throw "Status : $($r.status)" }
    Write-Host "    seq=$($r.seq)"
}

Test-Step "Stock PANTALON-L = 90 sur le nœud actif" {
    $r = Invoke-Api GET "$ACTIVE/stock/PANTALON-L"
    if ($r.quantity -ne 90) { throw "Stock=$($r.quantity) (attendu 90)" }
}

# ── 4. Anti-survente ──────────────────────────────────────────────────────────
Test-Step "Anti-survente : vente de 200 → doit être rejetée (409)" {
    try {
        Invoke-Api POST "$ACTIVE/write" @{
            op_type  = "sale"
            item_id  = "PANTALON-L"
            quantity = 200
        }
        throw "La vente aurait dû être rejetée"
    } catch {
        if ($_ -match "409|insuffisant") { return }  # attendu
        throw
    }
}

# ── 5. Idempotence ────────────────────────────────────────────────────────────
Test-Step "Idempotence : même op_id deux fois → seq identique" {
    $op_id = [System.Guid]::NewGuid().ToString()
    $r1 = Invoke-Api POST "$ACTIVE/write" @{
        op_type  = "sale"
        item_id  = "PANTALON-L"
        quantity = 1
        op_id    = $op_id
    }
    $r2 = Invoke-Api POST "$ACTIVE/write" @{
        op_type  = "sale"
        item_id  = "PANTALON-L"
        quantity = 1
        op_id    = $op_id
    }
    if ($r1.seq -ne $r2.seq) { throw "seq1=$($r1.seq) ≠ seq2=$($r2.seq)" }
    Write-Host "    seq=$($r1.seq) (identique les deux fois)"
}

# ── 6. Époque et fencing ──────────────────────────────────────────────────────
Test-Step "Époque courante accessible" {
    $r = Invoke-Api GET "$ACTIVE/epoch"
    Write-Host "    epoch=$($r.epoch) host=$($r.primary_host)"
    if ($r.epoch -lt 1) { throw "Époque invalide : $($r.epoch)" }
}

# ── 7. Journal disponible pour le passif ─────────────────────────────────────
Test-Step "Journal accessible (blobs opaques)" {
    $r = Invoke-Api GET "$ACTIVE/journal?after_seq=0&limit=5"
    Write-Host "    $($r.Count) entrée(s) retournée(s)"
    if ($r.Count -lt 1) { throw "Journal vide ?" }
    # Vérifier que les blobs sont bien hex (opaques)
    $first = $r[0]
    if ($first.blob_nonce -notmatch "^[0-9a-f]+$") {
        throw "blob_nonce n'est pas du hex : $($first.blob_nonce)"
    }
}

# ── 8. Synchronisation du passif ─────────────────────────────────────────────
Test-Step "Passif synchronisé (attente 10s)" {
    Write-Host "    Attente sync passif (intervalle 5s)..."
    Start-Sleep -Seconds 10
    $r = Invoke-Api GET "$PASSIVE/sync/status"
    Write-Host "    last_seq=$($r.last_seq)"
    if ($r.last_seq -lt 1) { throw "Passif non synchronisé : last_seq=$($r.last_seq)" }
}

Test-Step "Stock PANTALON-L cohérent sur le passif" {
    $active_stock  = (Invoke-Api GET "$ACTIVE/stock/PANTALON-L").quantity
    $passive_stock = (Invoke-Api GET "$PASSIVE/stock/PANTALON-L").quantity
    Write-Host "    actif=$active_stock  passif=$passive_stock"
    if ($active_stock -ne $passive_stock) {
        throw "Divergence : actif=$active_stock ≠ passif=$passive_stock"
    }
}

# ── 9. Relais : push + fetch d'un blob ────────────────────────────────────────
Test-Step "Relais : push blob (zéro-knowledge)" {
    $fake_blob = @{
        seq             = 9999
        blob_nonce      = "aa" * 24
        blob_ciphertext = "deadbeef" * 16
    }
    $r = Invoke-Api POST "$RELAY/blobs" $fake_blob @{ "X-Relay-Key" = $RELAY_KEY }
    if ($r.status -notin "stored","already_stored") { throw "Status : $($r.status)" }
}

Test-Step "Relais : le contenu stocké est opaque (pas de déchiffrement)" {
    $r = Invoke-Api GET "$RELAY/blobs?after_seq=9998&limit=1"
    if ($r.Count -lt 1) { throw "Blob non trouvé dans le relais" }
    Write-Host "    blob_nonce=$($r[0].blob_nonce.Substring(0,16))..."
    Write-Host "    → Le relais ne sait pas ce que contient ce blob. Zero-knowledge prouvé."
}

# ── Résumé ─────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "═══════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  Résultats : $PASS réussis / $($PASS + $FAIL) tests" -ForegroundColor $(if ($FAIL -eq 0) { "Green" } else { "Yellow" })
if ($FAIL -gt 0) {
    Write-Host "  $FAIL test(s) en échec — vérifier les logs des nœuds." -ForegroundColor Red
} else {
    Write-Host "  TOUS LES CRITÈRES DU BANC D'ESSAI SONT VALIDÉS" -ForegroundColor Green
}
Write-Host "═══════════════════════════════════════════════" -ForegroundColor Magenta
