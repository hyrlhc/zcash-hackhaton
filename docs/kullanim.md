# ZClaim — Çocuğa Anlatır Gibi + Kullanım Kılavuzu

**Tek cümle:** Cafe, “bu kişi bana en az 1 ZEC ödedi mi?” diye sorar. Sistem “evet” der. Kaç ZEC ödendiğini, cüzdanı, işlemi kimse görmez.

---

## 1. Çocuğa anlatır gibi

Düşün ki Alice, Quantum Cafe'de kahve içiyor ve **2,7 ZEC** ödüyor. Bu ödeme Zcash'te **gizli** (shielded): sokaktaki kimse tutarı, kimden kime gittiğini göremez.

Cafe'nin sadakat programı şunu merak eder:

> “Alice bize en az 1 ZEC verdi mi? O zaman bir kupon hak eder.”

Eski usul cevap: cüzdanı aç, faturayı göster, tutarı söyle. O zaman cafe **2,7**'yi de öğrenir. Fazladan bilgi sızmış olur.

ZClaim başka bir şey yapar. Alice'in cüzdanı bir **kanıt** üretir. Kanıt şunu söyler:

> “Evet, Quantum Cafe'ye giden gerçek bir gizli ödemem var ve tutarı 1 ZEC'den küçük değil.”

Cafe kanıtı kontrol eder ve **VALID** görür. Öğrendiği tek şey: **evet / hayır**.

- Tam tutar: **gizli**
- Cüzdan adresi: **gizli**
- İşlem detayı: **gizli**
- Başka ödemeler: **gizli**

Bu, “kapıyı açacak anahtarı göstermek” gibidir. Anahtarın kopyasını, evin planını veya cüzdanındaki parayı göstermezsin. Sadece “bu kapı bana ait” dersin.

### Neden Zcash?

Başka zincirlerde tutar zaten herkese açıktır. “Gizlemeden kanıtla” diye bir soru kalmaz. Zcash'te tutar zincirde gizli durur; ZClaim o gizliliği bozmadan **bir soruyu** cevaplar.

### Kötü cafe ne dener?

Cafe açgözlü olup şunu sorabilir:

```
en az 1 mi?  → evet
en az 2 mi?  → evet
en az 2,5 mi? → evet
en az 2,6 mi? → …
```

Bu, saklambaçta “sıcak / soğuk” oynamaktır. Bir süre sonra **tam tutar** ortaya çıkar.

Buna **Inference Guard** denir. Cüzdan, sorular birikip tutarı fazla daraltınca **dur** der. Üstelik kararı cevaba bakmadan verir: “hayır” demek de bilgi sızdırır, o yüzden tehlikeli soruyu hiç cevaplamaz.

### Ne gerçek, ne sahne?

- **Gerçek:** Zcash'in kendi kriptografisi, gerçek Halo2 kanıtı, gerçek doğrulama.
- **Sahne:** Demo ödemesi henüz testnet'e yazılmamış; not yerel üretilir. `--chain` ile canlı testnet kökü alınır ve demo'nun kendi kökü **reddedilir**. Bu dürüst sınırdır.

Sahte `TRUE` yok. Ekranda VALID görüyorsan arkasında kanıt vardır.

---

## 2. Bilgisayarda ne var?

```
zclaim/
├── crates/                 Rust motor
│   ├── zclaim-core         soru (predikat) ve bağlam
│   ├── zclaim-circuits     Halo2 devresi
│   ├── zclaim-proof        kanıt üret / doğrula
│   ├── zclaim-inference    Inference Guard
│   ├── zclaim-zcash        zincir kökü, Merkle tanık
│   ├── zclaim-protocol     Holder + Verifier
│   └── zclaim-wasm         tarayıcı için WASM
├── apps/
│   ├── quantum-cafe        terminal demosu
│   └── verifier-demo       tarayıcı demosu
├── packages/verifier-sdk   TypeScript SDK
└── docs/                   araştırma, tehdit modeli, bu kılavuz
```

Gerekenler: **Rust 1.97.1** (`rust-toolchain.toml` pinler), **Node.js** (web demo için). `protoc` gerekmez.

---

## 3. Kullanım kılavuzu

### A. Testler (her şey çalışıyor mu?)

```bash
cd /Users/halocline/Desktop/zcash_hackhaton

cargo test --workspace --release
cargo test --workspace --release --features zclaim-zcash/lightwalletd
cargo clippy --workspace --all-targets -- -D warnings
```

İlk `release` derlemesi kanıtlama anahtarını üretir; birkaç dakika sürebilir.

### B. Terminal demosu (jüriye bu)

```bash
cargo run --release -p quantum-cafe
```

Canlı Zcash testnet kökleriyle:

```bash
cargo run --release -p quantum-cafe -- --chain
```

Ne göreceksin, sırayla:

1. **Dürüst soru** → `VALID`, tutar HIDDEN
2. **Başka uygulama** → aynı yanıt cafe'de geçmez
3. **Açgözlü sorular** → `>= 2.6` BLOCKED
4. `--chain` varsa gerçek kök ACCEPTED, demo kökü REFUSED
5. Cafe'nin öğrendiği: “2.5 ZEC veya daha fazla”; gerçek tutar 2.7, cüzdanda kaldı

Ağ yoksa `--chain` olmadan 1–3 yeter.

### C. Tarayıcı demosu

WASM bir kez üretilmiş olmalı (`packages/verifier-sdk/wasm/`). Yoksa:

```bash
cargo install wasm-pack
cd crates/zclaim-wasm
wasm-pack build --release --target web \
  --out-dir ../../packages/verifier-sdk/wasm --out-name zclaim
```

Sonra:

```bash
cd apps/verifier-demo
npm install
npm run dev
```

Tarayıcı: [http://localhost:5173](http://localhost:5173)

- **En az 1 ZEC ödendi mi?** → gerçek kanıt, VALID
- **Başka uygulamada dene** → replay başarısız
- Eşik butonları → Guard bir noktada BLOCKED

İlk yüklemede anahtar üretimi biraz sürer. Takılırsa bekleyin; çökme değil.

### D. TypeScript SDK (üçüncü taraf)

Bir uygulama yalnızca şunu ister: “bu kullanıcı Quantum Cafe'ye ≥ 1 ZEC ödedi mi?”

```ts
import { start, Verifier, stagedMerchant, QUANTUM_CAFE_SEED, ZEC } from "@zclaim/verifier-sdk";
import { Wallet } from "@zclaim/verifier-sdk/wallet";

await start();

const merchant = stagedMerchant("quantum-cafe", QUANTUM_CAFE_SEED);
const wallet = Wallet.staged(); // demo: yerel not
const verifier = new Verifier("loyalty.quantum-cafe.example", "ironwood");

// Gerçek hayatta kök zincirden gelir. Demo'da sahne kökü:
verifier.observeRoot(wallet.stagedAnchorHex(), 3_500_000);

const request = verifier.createProofRequest(
  { merchant, amount: { operator: "GTE", value: ZEC } },
  "loyalty-tier",
  3_500_100,
);

const answer = wallet.respond(request, 3_500_000);
if (answer.status !== "ANSWER") throw new Error(answer.message);

const accepted = verifier.verifyProof(request, answer.presentation);
// accepted.nullifier  → bu cafe'de bu ödeme
// accepted.holderTag  → bu cafe'de bu kişi (başka uygulamada başka)
```

API'nin tamamı:

| Fonksiyon | Ne işe yarar |
|---|---|
| `createProofRequest` | Taze nonce'lu soru yayımlar |
| `verifyProof` | İstek uyumu + kök + Halo2 + tekrar kullanım |
| `verifyNullifier` | Bu ödeme burada daha önce kullanıldı mı? |

Kökü **kanıtı getiren kişiden** alma. Kök, senin zincir görüşünden gelmeli. Aksi halde biri kendi ağacını uydurup her şeyi “kanıtlar”.

---

## 4. Sık sorular

**Bu mock mu?** Hayır. Halo2 IPA kanıtı, Zcash'in `NoteCommit` ve `MerkleCRH` aygıtları. Mock proving yok.

**Neden demo notu zincirde değil?** Testnet bakiyesi ve cüzdan taraması lazım; kriptografi değil lojistik. `--chain` bunu saklamaz: gerçek kök kabul, bizim kök red.

**Kanıt “ben ödedim” diyor mu?** “Bu ödemenin şifresini çözebilen ve `holder_sk` sahibi kişi, koşulun doğru olduğunu söylüyor.” Hem gönderen hem satıcı çıktıyı çözebilir. Bu sınır `docs/architecture-decision.md` içinde yazıyor.

**Inference Guard kriptografi mi?** Hayır, politika. Cüzdan kapatılırsa çalışmaz. O yüzden cüzdanda durur, cafe'de değil.

---

## 5. Jüriye 60 saniye

1. Terminal veya tarayıcı demosunu aç.
2. VALID'i göster: tutar HIDDEN.
3. Replay'i göster: başka uygulamada geçmez.
4. `>= 2.6` BLOCKED: “soruya bakarak duruyor, cevaba değil.”
5. İsteğe bağlı `--chain`: gerçek testnet kökü, sahte kök red.
6. Kapanış: **Don't ask for the data. Ask for the proof.**

Daha fazla: `docs/demo.md`, `docs/threat-model.md`, `docs/architecture-decision.md`.
