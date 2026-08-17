import init, {
  Wallet,
  Verifier,
  warmUp,
  stagedMerchant,
  formatZec,
} from "../../packages/verifier-sdk/wasm/zclaim.js";

const ZEC = 100_000_000;
const PAID = String((27 * ZEC) / 10);
const QUANTUM_CAFE = 0x51;
const TIP_FALLBACK = 3_500_000;
const DOMAIN = "loyalty.quantum-cafe.example";

const $ = (id) => document.getElementById(id);

let chain = null;
let wasmReady = false;
let zclaimOn = false;
let alice = null;
let loyalty = null;
let merchant = null;
let lastRequest = null;
let lastPresentation = null;

function shortHex(hex) {
  if (!hex || hex.length < 16) return hex || "—";
  return `${hex.slice(0, 8)}…${hex.slice(-6)}`;
}

function setDl(rows) {
  $("chain-dl").innerHTML = rows
    .map(([k, v]) => `<div><dt>${k}</dt><dd>${v}</dd></div>`)
    .join("");
}

async function loadChain() {
  $("chain-badge").textContent = "bağlanıyor";
  setDl([["Durum", "testnet.zec.rocks bekleniyor"]]);
  try {
    const res = await fetch("/api/chain");
    const body = await res.json();
    if (!res.ok) throw new Error(body.error || res.statusText);
    chain = body;
    $("chain-badge").textContent = "gerçek testnet";
    $("chain-badge").classList.add("ok");
    setDl([
      ["RPC", body.endpoint],
      ["Para transferi", "Yok. Cüzdan yok. Yalnızca okuma."],
      ["Bu yerel mi?", "Hayır. Public Zcash testnet."],
      ["Sunucu", body.serverVersion],
      ["Ağ", body.chain === "test" ? "testnet" : body.chain],
      ["Blok yüksekliği", String(body.tipHeight)],
      ["Blok özeti", body.blockHash],
      ["Korumalı havuzdaki kayıt", `${body.orchardLeaves} adet (kim olduğu yazmaz)`],
      ["Ağaç kökü", body.orchardAnchor],
      ["Gizli kalan", "kim, kime, tutar, not"],
    ]);
  } catch (err) {
    $("chain-badge").textContent = "zincir yok";
    setDl([
      ["Hata", err.message],
      ["Ne yap", "Başka bir terminalde: cargo run -p chain-api --release"],
    ]);
  }
}

async function loadWasm() {
  await init();
  warmUp();
  merchant = JSON.parse(stagedMerchant("quantum-cafe", QUANTUM_CAFE));
  alice = Wallet.staged(
    JSON.stringify({
      paidZatoshi: PAID,
      merchantSeed: QUANTUM_CAFE,
      noteSeed: 0x09,
      holderSeed: 0x11,
    }),
  );
  loyalty = new Verifier(DOMAIN, "ironwood", 100);
  loyalty.observeRoot(alice.stagedAnchorHex(), chain?.tipHeight ?? TIP_FALLBACK);
  wasmReady = true;
}

function render() {
  $("mode-label").textContent = zclaimOn ? "Şu an: ZClaim açık" : "Şu an: sadece Zcash";
  const tip = chain?.tipHeight ?? "—";
  const root = chain ? shortHex(chain.orchardAnchor) : "—";

  if (!zclaimOn) {
    $("buyer-hint").textContent =
      "Ödemen zincirde gizli. Dükkâna gösterecek bir kanıtın yok. Kapı kör.";
    $("seller-hint").textContent =
      "Kasana kim geldiğini zincirden okuyamazsın. Viewing key istersen tutar da açılır.";
    $("buyer-facts").innerHTML = `
      <li>Ağdaki son blok: ${tip} (gerçek)</li>
      <li>Kök: ${root} (gerçek)</li>
      <li>2,7 ZEC ve cüzdanın bu listede yok — Zcash yayımlamıyor</li>`;
    $("seller-facts").innerHTML = `
      <li>Aynı kök: ${root}</li>
      <li>Gelen gizli ödemeler burada sıralanamaz</li>
      <li>Bu yüzden “ödedin mi?” sorusuna zincir cevap vermez</li>`;
    $("buyer-actions").innerHTML = "";
    $("seller-actions").innerHTML = "";
    $("buyer-out").textContent =
      "ZClaim kapalı. Yapacak bir düğme yok: gizlilik var, kapı yok.";
    $("seller-out").textContent =
      "ZClaim kapalı. Satıcı alıcıyı zincirden okuyamaz.";
    return;
  }

  $("buyer-hint").textContent =
    "1. adımsın. Kanıt üret. Ekrana 2,7 yazılmaz.";
  $("seller-hint").textContent =
    "2. adım: kontrol et. Sonra başka dükkân ve eşik butonları.";
  $("buyer-facts").innerHTML = `
    <li>Demo ödemesi: 2,7 ZEC (bu cihazda, testnet’e yazılmadı)</li>
    <li>Gerçek ağ hâlâ blok ${tip}</li>
    <li>Gerçek kök ${root} — senin kanıtın bunun altında değil</li>`;
  $("seller-facts").innerHTML = `
    <li>Soru: bu dükkâna en az 1 ZEC gitti mi?</li>
    <li>Cüzdan adresi istenmez</li>
    <li>Aynı kanıt başka dükkânda geçmez</li>`;

  $("buyer-actions").innerHTML =
    `<button type="button" id="btn-prove">1. Kanıt üret</button>`;
  $("seller-actions").innerHTML = `
    <button type="button" id="btn-verify">2. Kanıtı kontrol et</button>
    <button type="button" id="btn-replay">3. Başka dükkânda dene</button>
    <button type="button" id="btn-1">en az 1 ZEC?</button>
    <button type="button" id="btn-2">en az 2?</button>
    <button type="button" id="btn-25">en az 2,5?</button>
    <button type="button" id="btn-26">en az 2,6?</button>
    <button type="button" id="btn-27">en az 2,7?</button>
    <button type="button" id="btn-root">4. Kökleri karşılaştır</button>`;

  $("buyer-out").textContent = wasmReady
    ? "Hazır. “Kanıt üret”e bas."
    : "Kanıt motoru yükleniyor, bir dakika sürebilir…";
  $("seller-out").textContent = "Önce soldan kanıt gelsin.";

  const on = (id, fn) => {
    const el = $(id);
    if (el) el.addEventListener("click", fn);
  };
  on("btn-prove", prove);
  on("btn-verify", verifyLast);
  on("btn-replay", replay);
  on("btn-1", () => probe(ZEC));
  on("btn-2", () => probe(2 * ZEC));
  on("btn-25", () => probe((25 * ZEC) / 10));
  on("btn-26", () => probe((26 * ZEC) / 10));
  on("btn-27", () => probe((27 * ZEC) / 10));
  on("btn-root", compareRoots);
}

function atLeast(zat) {
  return { merchant, amount: { operator: "GTE", value: zat } };
}

function tip() {
  return chain?.tipHeight ?? TIP_FALLBACK;
}

function prove() {
  if (!wasmReady) return;
  lastRequest = JSON.parse(
    loyalty.request(JSON.stringify(atLeast(ZEC)), "loyalty-tier", tip() + 100),
  );
  const response = JSON.parse(alice.respond(JSON.stringify(lastRequest), tip()));
  if (response.status !== "ANSWER") {
    $("buyer-out").textContent = response.message ?? "Cüzdan reddetti.";
    lastPresentation = null;
    return;
  }
  lastPresentation = response.presentation;
  $("buyer-out").textContent = [
    "Kanıt hazır. Tutar bu kutuda yok.",
    `Boyut: ${lastPresentation.proof.length / 2} bayt`,
    `Soru: en az ${formatZec(String(ZEC))} ZEC`,
    `Bu dükkâna özel etiket: ${shortHex(lastPresentation.statement.nullifier)}`,
    "",
    "Şimdi sağda “Kanıtı kontrol et”.",
  ].join("\n");
}

function verifyLast() {
  if (!lastRequest || !lastPresentation) {
    $("seller-out").textContent = "Önce solda kanıt üret.";
    return;
  }
  try {
    const accepted = JSON.parse(
      loyalty.accept(JSON.stringify(lastRequest), JSON.stringify(lastPresentation)),
    );
    $("seller-out").textContent = [
      "Geçerli.",
      `Koşul: en az ${formatZec(String(ZEC))} ZEC`,
      `Kök: ${shortHex(accepted.anchor)}`,
      "",
      "Tam tutar: gizli",
      "Cüzdan: gizli",
    ].join("\n");
  } catch (err) {
    $("seller-out").textContent = String(err.message ?? err);
  }
}

function replay() {
  if (!wasmReady) return;
  const other = new Verifier("insurer.example", "ironwood", 100);
  other.observeRoot(alice.stagedAnchorHex(), tip());
  const theirs = JSON.parse(
    other.request(JSON.stringify(atLeast(ZEC)), "underwriting", tip() + 100),
  );
  const response = JSON.parse(alice.respond(JSON.stringify(theirs), tip()));
  if (response.status !== "ANSWER") {
    $("seller-out").textContent = response.message ?? "Red.";
    return;
  }
  const cafeReq = JSON.parse(
    loyalty.request(JSON.stringify(atLeast(ZEC)), "loyalty-tier", tip() + 100),
  );
  try {
    loyalty.accept(JSON.stringify(cafeReq), JSON.stringify(response.presentation));
    $("seller-out").textContent = "Hata: başka dükkânın kanıtı kabul edildi.";
  } catch (err) {
    $("seller-out").textContent =
      "Başka dükkânın kanıtı burada geçmedi.\n" + String(err.message ?? err);
  }
}

function probe(zat) {
  if (!wasmReady) return;
  const request = JSON.parse(
    loyalty.request(JSON.stringify(atLeast(zat)), "loyalty-tier", tip() + 100),
  );
  const response = JSON.parse(alice.respond(JSON.stringify(request), tip()));
  const q = `en az ${formatZec(String(zat))} ZEC`;
  if (response.status === "ANSWER") {
    $("seller-out").textContent = [
      `${q} → evet`,
      response.decision?.resulting?.describe ?? "",
      response.decision?.reason ?? "",
    ]
      .filter(Boolean)
      .join("\n");
  } else {
    $("seller-out").textContent = `${q} → durdu\n${response.message ?? ""}`;
  }
}

function compareRoots() {
  $("seller-out").textContent = [
    `Demo kanıtının kökü: ${alice ? shortHex(alice.stagedAnchorHex()) : "—"}`,
    `Testnet kökü:        ${chain ? shortHex(chain.orchardAnchor) : "ağ yok"}`,
    "",
    "Aynı değiller. Kanıt henüz gerçek bir testnet ödemesine bağlı değil.",
    "Üstteki kutu gerçek ağı okuyor; bu buton o farkı gösteriyor.",
  ].join("\n");
}

$("mode").addEventListener("change", (e) => {
  zclaimOn = e.target.checked;
  render();
});
$("theme").addEventListener("click", () => {
  const html = document.documentElement;
  const dark = html.getAttribute("data-theme") !== "dark";
  html.setAttribute("data-theme", dark ? "dark" : "light");
  $("theme").textContent = dark ? "Açık tema" : "Koyu tema";
});

render();
loadChain()
  .then(loadWasm)
  .then(render)
  .catch((err) => {
    $("buyer-out").textContent = String(err);
  });
