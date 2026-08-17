import init, { Wallet, Verifier, warmUp, stagedMerchant, formatZec } from "../../packages/verifier-sdk/wasm/zclaim.js";

const ZEC = 100_000_000, PAID = String(2.7 * ZEC), START_A = 5 * ZEC, SEND = 2.7 * ZEC;
const DOMAIN = "loyalty.quantum-cafe.example", TIP_FALLBACK = 3_500_000;
const $ = (id) => document.getElementById(id);
let chain, wallet, verifier, merchant, request = null, presentation = null;
let wasmReady = false, zclaimOn = false, selected = ZEC, queryCount = 0;
let aBalance = START_A, bBalance = 0, sent = false, txs = [];
const thresholds = [1, 2, 2.5, 2.6, 2.7, 3];
const shortHex = (v) => v?.length > 18 ? `${v.slice(0, 9)}…${v.slice(-7)}` : v || "—";
const height = () => chain?.tipHeight ?? TIP_FALLBACK;
const predicate = (amount) => ({ merchant, amount: { operator: "GTE", value: amount } });
const zecLabel = (zatoshi) => `${(Number(zatoshi) / ZEC).toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ZEC`;
const clock = () => new Date().toLocaleTimeString("tr-TR", { hour: "2-digit", minute: "2-digit" });

function setResult(id, text, tone = "") { const node = $(id); node.textContent = text; node.className = `result-box ${tone}`; }
function log(actor, message) {
  const row = document.createElement("div");
  row.innerHTML = `<time>${clock()}</time><span>${actor}</span><p>${message}</p>`;
  $("event-log").prepend(row);
  while ($("event-log").children.length > 4) $("event-log").lastElementChild.remove();
}
function setChainRows(rows) { $("chain-dl").innerHTML = rows.map(([k, v]) => `<div><dt>${k}</dt><dd>${v}</dd></div>`).join(""); }

function renderLedger(id, rows) {
  $(id).innerHTML = [0, 1, 2].map((i) => {
    const tx = rows[i];
    return tx
      ? `<div class="ledger-row"><span>${tx.time}</span><span>${tx.label}</span><span>${tx.amount}</span></div>`
      : `<div class="ledger-row muted"><span>${i === 0 && !rows.length ? "·" : ""}</span><span>${i === 0 && !rows.length ? "Kayıt yok" : ""}</span><span></span></div>`;
  }).join("");
}

function paintWallets() {
  $("a-balance").textContent = zecLabel(aBalance);
  $("a-tag").textContent = zecLabel(aBalance);
  $("b-balance").textContent = zecLabel(bBalance);
  $("b-tag").textContent = zecLabel(bBalance);
  $("a-last").textContent = sent ? "A → B · 2,70 ZEC" : "—";
  $("send-ab").disabled = sent;
  $("send-ab").textContent = sent ? "GÖNDERİLDİ · 2,7 ZEC" : "B'YE 2,7 ZEC GÖNDER";
  renderLedger("a-ledger", txs.map((tx) => ({ ...tx, label: "A → B" })));
  renderLedger("b-ledger", txs.map((tx) => ({ ...tx, label: "B ← A" })));
}

async function loadChain() {
  try {
    const response = await fetch("/api/chain"), body = await response.json();
    if (!response.ok) throw new Error(body.error || response.statusText);
    chain = body;
    $("chain-badge").textContent = "ZCASH TESTNET / LIVE";
    $("network-pill").classList.add("ok");
    $("tip-value").textContent = `Blok ${body.tipHeight.toLocaleString("tr-TR")} · canlı`;
    setChainRows([["Ağ", body.chain === "test" ? "Zcash testnet" : body.chain], ["Blok", body.tipHeight.toLocaleString("tr-TR")], ["Orchard yaprağı", body.orchardLeaves.toLocaleString("tr-TR")], ["Ağaç kökü", shortHex(body.orchardAnchor)], ["RPC", body.endpoint]]);
  } catch (error) {
    $("chain-badge").textContent = "CHAIN API / OFFLINE";
    $("tip-value").textContent = "Zincir servisi çevrimdışı";
    setChainRows([["Başlat", "cargo run -p chain-api --release"], ["Hata", error.message]]);
  }
}

function attachPayment() {
  wallet = Wallet.staged(JSON.stringify({ paidZatoshi: PAID, merchantSeed: 0x51, noteSeed: 0x09, holderSeed: 0x11 }));
  verifier.observeRoot(wallet.stagedAnchorHex(), height());
}

function bootVerifier() {
  merchant = JSON.parse(stagedMerchant("quantum-cafe", 0x51));
  verifier = new Verifier(DOMAIN, "ironwood", 100);
}

function createSession() {
  bootVerifier();
  wallet = request = presentation = null;
  queryCount = 0;
  aBalance = START_A;
  bBalance = 0;
  sent = false;
  txs = [];
}

async function loadProofEngine() {
  await init();
  warmUp();
  bootVerifier();
  wasmReady = true;
  if (sent) attachPayment();
}

function renderThresholds() {
  $("seller-actions").innerHTML = thresholds.map((v) => `<button type="button" data-zec="${v}" class="${selected === v * ZEC ? "selected" : ""}">≥ ${String(v).replace(".", ",")}</button>`).join("") + '<button type="button" id="send-request" class="primary">SORGU GÖNDER →</button>';
  $("query-value").textContent = wasmReady ? `${formatZec(String(selected))} ZEC` : `${(selected / ZEC).toLocaleString("tr-TR", { minimumFractionDigits: 1 })} ZEC`;
  document.querySelectorAll("[data-zec]").forEach((button) => button.addEventListener("click", () => { selected = Number(button.dataset.zec) * ZEC; renderThresholds(); }));
  $("send-request").addEventListener("click", issueRequest);
}

function render() {
  document.body.classList.toggle("zclaim-on", zclaimOn);
  $("mode-label").textContent = zclaimOn ? "Zcash + ZClaim" : "Yalnızca Zcash";
  $("mode-description").textContent = zclaimOn ? "Uygulama yalnızca koşul sonucunu öğrenir; özel alanlar cüzdanda kalır." : "Zincir gizli; fakat uygulama ödeme koşulunu doğrulayamıyor.";
  $("channel-label").textContent = zclaimOn ? "ZK PROOF CHANNEL" : "KANAL KAPALI";
  $("guard-badge").textContent = zclaimOn ? "AKTİF" : "PASİF";
  $("pane-buyer").classList.toggle("active", zclaimOn); $("pane-seller").classList.toggle("active", zclaimOn);
  $("wallet-state").textContent = sent ? (zclaimOn ? (wasmReady ? "ÖDENDİ" : "YÜKLENİYOR") : "ÖDENDİ") : (zclaimOn ? (wasmReady ? "HAZIR" : "YÜKLENİYOR") : "HAZIR");
  $("app-state").textContent = zclaimOn ? (sent ? "SORGU HAZIR" : "ÖDEME YOK") : "KÖR";
  $("buyer-facts").innerHTML = "<span>Tutar gizli</span><span>Adres gizli</span>";
  $("seller-facts").innerHTML = "<span>Boolean sonuç</span><span>Domain bağlı</span>";
  paintWallets();
  renderThresholds();
  if (!zclaimOn) {
    $("buyer-hint").textContent = sent
      ? "2,70 ZEC B’ye gitti. ZClaim kapalı; uygulamaya kanıt yok."
      : "Bakiye sitede durur. B’ye gönder; dış cüzdan yok, zincire yayın yok.";
    $("seller-hint").textContent = sent
      ? "Gelen kayıt cüzdanda görünür. Uygulama hâlâ zincirden tutarı okuyamaz."
      : "Gönderim olsa bile uygulama koşulu doğrulayamaz.";
    setResult("buyer-out", sent ? "Ödeme yerel cüzdanlarda. Paylaşılabilir doğrulama yok." : "A: 5,00 ZEC · B: 0,00 ZEC");
    setResult("seller-out", sent ? "UNKNOWN — public zincirden cevap alınamıyor." : "Ödeme bekleniyor.");
    return;
  }
  $("buyer-hint").textContent = sent
    ? "Sağdan gelen koşulu guard tarar; güvenliyse cüzdan gerçek Halo2 kanıtını üretir."
    : "Önce B’ye 2,70 ZEC gönder. Kanıt, o yerel ödemenin üzerine basılır.";
  $("seller-hint").textContent = sent
    ? "Bir eşik seç ve cüzdana gönder. Yanıt yalnızca TRUE, FALSE veya BLOCKED olur."
    : "Ödeme geldikten sonra eşik sor. Tutar ekranın bu tarafında sorgunun cevabı değildir.";
  setResult("buyer-out", !wasmReady ? "Halo2 kanıt motoru hazırlanıyor…" : sent ? "Güvenli sorgu bekleniyor." : "Gönderimden sonra kanıt üretilebilir.");
  setResult("seller-out", sent ? "Bir eşik seçip sorguyu gönder." : "A’dan ödeme bekleniyor.");
}

function sendAB() {
  if (sent) return;
  sent = true;
  aBalance -= SEND;
  bBalance += SEND;
  txs = [{ time: clock(), amount: "2,70 ZEC" }];
  if (wasmReady) attachPayment();
  $("packet").textContent = zclaimOn ? "1 BIT" : "GİZLİ";
  render();
  log("Cüzdan A", "2,70 ZEC B’ye gönderildi. Zincire yayın yok.");
}

function issueRequest() {
  if (!zclaimOn || !wasmReady) return setResult("seller-out", "Önce ZClaim’i aç ve kanıt motorunu bekle.", "blocked");
  if (!sent || !wallet) return setResult("seller-out", "Önce A’dan B’ye 2,70 ZEC gönder.", "blocked");
  request = JSON.parse(verifier.request(JSON.stringify(predicate(selected)), "loyalty-tier", height() + 100)); presentation = null;
  $("wallet-state").textContent = "İSTEK GELDİ"; $("app-state").textContent = "YANIT BEKLİYOR";
  setResult("seller-out", `REQUEST SENT\namount >= ${formatZec(String(selected))} ZEC\nBeklenen çıktı: boolean proof`);
  setResult("buyer-out", `Yeni sorgu: ödeme ≥ ${formatZec(String(selected))} ZEC mi?\nGuard kontrolü kanıt üretmeden önce çalışacak.`);
  log("Cüzdan B", `≥ ${formatZec(String(selected))} ZEC koşulunu gönderdi.`);
}

function prove() {
  if (!zclaimOn) return setResult("buyer-out", "Önce sağ üstteki ZClaim anahtarını aç.", "blocked");
  if (!wasmReady) return setResult("buyer-out", "Kanıt motoru hâlâ yükleniyor. Birkaç saniye bekle.", "blocked");
  if (!sent || !wallet) return setResult("buyer-out", "Önce B’ye 2,70 ZEC gönder.", "blocked");
  if (!request) return setResult("buyer-out", "Önce sağ taraftan bir sorgu gönder.", "blocked");
  const response = JSON.parse(wallet.respond(JSON.stringify(request), height())); queryCount++;
  $("guard-meter").style.width = `${Math.min(100, queryCount * 20)}%`;
  if (response.status !== "ANSWER") {
    presentation = null;
    if (response.refusal === "CLAIM_IS_FALSE") {
      $("wallet-state").textContent = "FALSE"; $("app-state").textContent = "FALSE"; $("packet").textContent = "FALSE";
      $("guard-badge").textContent = "SAFE"; $("guard-knowledge").textContent = "Yanlış koşul kaydedildi";
      setResult("buyer-out", "FALSE\nKoşul sağlanmıyor; kanıt üretilemez.", "blocked");
      setResult("seller-out", "FALSE · WALLET RESPONSE\nNot: Mevcut protokolde FALSE bir ZK proof değil, cüzdan reddidir.", "blocked");
      log("Cüzdan A", "Koşul FALSE döndü; bu dal kriptografik sunum üretmiyor."); return;
    }
    $("wallet-state").textContent = "BLOKLADI"; $("guard-badge").textContent = "BLOCK"; $("guard-knowledge").textContent = "Gizlilik eşiği korundu";
    setResult("buyer-out", `BLOCKED\n${response.message || "Inference Guard sorguyu reddetti."}`, "blocked"); setResult("seller-out", "BLOCKED — cüzdan bu sorguya cevap vermedi.", "blocked"); log("Guard", "Sorgu, olası bilgi sızıntısı nedeniyle kanıttan önce kesildi."); return;
  }
  presentation = response.presentation; $("wallet-state").textContent = "KANIT HAZIR"; $("guard-badge").textContent = response.decision?.status || "SAFE"; $("guard-knowledge").textContent = response.decision?.resulting?.describe || "Sınır içinde";
  setResult("buyer-out", `PROOF READY · ${(presentation.proof.length / 2).toLocaleString("tr-TR")} byte\nTutar: gizli · Adres: gizli\nNullifier: ${shortHex(presentation.statement.nullifier)}`, "success"); setResult("seller-out", "Kriptografik sunum geldi. Soldaki “Doğrula” ile verifier kontrolünü çalıştır."); log("Cüzdan A", "Guard izin verdi; domain'e bağlı kanıt üretildi.");
}

function verify() {
  if (!zclaimOn) return setResult("seller-out", "Önce sağ üstteki ZClaim anahtarını aç.", "blocked");
  if (!request || !presentation) return setResult("seller-out", "Doğrulanacak bir kanıt yok. Sırayla: gönder → sorgu → kanıt.", "blocked");
  try {
    const accepted = JSON.parse(verifier.accept(JSON.stringify(request), JSON.stringify(presentation)));
    $("app-state").textContent = "TRUE"; $("packet").textContent = "TRUE";
    setResult("seller-out", `TRUE · PROOF VERIFIED\nKoşul: amount >= ${formatZec(String(selected))} ZEC\nAnchor: ${shortHex(accepted.anchor)}\nTam tutar ve cüzdan kimliği açıklanmadı.`, "success"); log("Verifier", "Kanıt ve public statement doğrulandı: TRUE.");
  } catch (error) { $("app-state").textContent = "REJECTED"; setResult("seller-out", `REJECTED\n${error.message || error}`, "blocked"); }
}

function compareRoots() {
  setResult("seller-out", `LOCAL PROOF ROOT  ${wallet ? shortHex(wallet.stagedAnchorHex()) : "gönderim yok"}\nLIVE TESTNET ROOT ${chain ? shortHex(chain.orchardAnchor) : "chain API çevrimdışı"}\n\nEşleşmiyor: kriptografi gerçek, A→B notu yerel.`, "blocked");
  log("Sistem", "Yerel kanıt kökü ile public testnet kökü karşılaştırıldı.");
}

$("mode").addEventListener("change", (event) => { zclaimOn = event.target.checked; render(); log("Sistem", zclaimOn ? "ZClaim katmanı etkinleştirildi." : "Yalnızca Zcash moduna dönüldü."); });
$("reset").addEventListener("click", () => { if (wasmReady) createSession(); else { aBalance = START_A; bBalance = 0; sent = false; txs = []; wallet = request = presentation = null; } selected = ZEC; $("guard-meter").style.width = "0"; $("guard-knowledge").textContent = "Bilgi sızıntısı yok"; $("packet").textContent = "1 BIT"; render(); log("Sistem", "Oturum ve guard geçmişi sıfırlandı."); });
$("btn-root").addEventListener("click", compareRoots);
$("send-ab").addEventListener("click", sendAB);
$("prove").addEventListener("click", prove);
$("verify").addEventListener("click", verify);
paintWallets();
render();
loadChain().then(loadProofEngine).then(render).catch((error) => setResult("buyer-out", `Proof engine error: ${error.message || error}`, "blocked"));
