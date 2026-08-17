# ZClaim

**Veriyi isteme. Kanıtı iste.**

ZClaim, bir Zcash korumalı ödemesinin belirli bir koşulu sağladığını, ödemenin
kendisini açığa çıkarmadan kanıtlar.

Bir kullanıcı Quantum Cafe'ye 2,7 ZEC öder. Doğrulayıcı sorar: *"Quantum Cafe'ye
en az 1 ZEC ödedin mi?"* ZClaim `TRUE` yanıtını verir. Doğrulayıcı tutarı,
adresi, işlemi veya başka bir ödemeyi öğrenmez — ve eşiği yükselterek tekrar
tekrar sorup tutarı çıkarmaya çalıştığında cüzdan yanıt vermeyi bırakır.

```
cargo run --release -p quantum-cafe
```

Tarayıcı:

```
cd apps/verifier-demo && npm install && npm run dev
```

Anlatım ve kullanım: [`docs/kullanim.md`](docs/kullanim.md).

---

## İki iddia

**1. Tek soru, tek bit.** Kanıt, gerçek bir Orchard notu üzerinde gerçek bir
Halo2 kanıtıdır ve Zcash'in kendi not taahhüdü ile Merkle aygıtlarını kullanır.
Satıcı imzalı bir credential yok, güvenilen bir issuer yok, oracle yok.

**2. Soru dizisi de bir saldırıdır.** `>= 1`, `>= 2`, `>= 2,5`, `>= 2,6`
sorularının her biri tek başına sağlamdır; birlikte, tutarı gizlemek için
kurulmuş bir sistemden o tutarı ikili aramayla çıkarırlar. **Inference Guard**,
yanıtların toplamda ne anlama geldiğini takip eder ve aralık bir tabanın altına
daralmadan önce reddeder. Cüzdan tarafında çalışır, çünkü saldırgan
doğrulayıcıdır.

Bunu bir devreden fazlası yapan şey ikincisidir.

---

## Durum

`cargo test --workspace --release` — **85 test geçiyor**; bunlara yalnızca
`MockProver` değil gerçek IPA kanıtları ve Zcash testnet ağaç durumunun canlı
okunması da dahil.

| | |
|---|---|
| Not taahhüdü | Zcash'in kendi `NoteCommit^Orchard` aygıtı, değiştirilmeden |
| Ağaç üyeliği | Zcash'in kendi `MerkleCRH^Orchard`'ı, 32 seviye |
| Kanıt sistemi | Pallas/Vesta üzerinde `halo2_proofs` IPA, güvenilir kurulum yok, `k = 11` |
| Havuzlar | Orchard ve Ironwood (NU6.3) — tek devre ikisini de kapsıyor |
| Zincir okuma | Canlı; `testnet.zec.rocks:443`'e karşı light wallet gRPC ile |
| Zincir yazma | **Yok.** Henüz açık bir zincirde bize ait bir not bulunmuyor |

Son satır dürüst uyarıdır ve demo bunu kendi ilk ekranında söyler. Testler ve
demo gerçek bir Orchard taahhüt ağacı kurar, ama yerel olarak. Zincir üstünde
bir tane üretmek testnet fonu ve bir cüzdan taraması gerektirir — kriptografi
değil, lojistik. `--chain` ile çalıştırın ve zincire bağlı bir doğrulayıcının
demonun kendi kökünü reddedişini izleyin.

Depoda hiçbir yerde sahte (mock) kanıt modu yoktur.

---

## Beyan

Özel tanık, ispatlayıcının dışına asla çıkmaz:

```
g_d, pk_d       alıcı adresinin eğri noktaları
v               zatoshi cinsinden not değeri
rho, psi, rcm   not rastgeleliği
path, pos       not taahhüt ağacındaki doğrulama yolu
holder_sk       nottan bağımsız, uzun ömürlü cüzdan sırrı
```

Açık girdiler, doğrulayıcının gördüğü her şey:

```
anchor                          not taahhüt ağacı kökü
merchant g_d.x, g_d.y,
         pk_d.x, pk_d.y         satıcının eksiksiz Orchard alıcı adresi
threshold, direction            eşik ve karşılaştırmanın yönü
domain_tag                      H(doğrulayıcı alanı)
nullifier    = Poseidon(psi, domain_tag)
holder_tag   = Poseidon(holder_sk, domain_tag)
context      = H(alan, nonce, predikat, amaç, son geçerlilik)
```

> `anchor` altında taahhüt edilmiş, `(g_d, pk_d)` alıcısına ödeme yapan ve değeri
> `threshold` karşılaştırmasını sağlayan bir Orchard notu biliyorum; yayımlanan
> etiketler o nottan ve elimdeki bir sırdan türetildi, ikisi de bu doğrulayıcıya
> kapsanmış durumda; ve tamamı `context`'e bağlı.

`holder_sk` dışındaki her şey, ispatlayıcının şifresini çözebildiği bir nottan
elde edilebilir — alıcı olarak `ivk` ile veya gönderen olarak `ovk` ile.

## Neden Zcash

Beyan *Zcash konsensüs durumu hakkındadır*. Anchor bir Zcash taahhüt ağacı
köküdür; taahhüt, korumalı bir not üzerindeki Sinsemilla taahhüdüdür. Şeffaf bir
zincirde tutar zaten açıktır, dolayısıyla soru anlamsızdır; not taahhüt ağacı
olmayan bir zincirde ise Merkle kanıtı verilecek bir şey yoktur. Mahremiyeti
ZClaim eklemiyor — o Zcash'in; ZClaim onu feda etmeden soru yanıtlamanın yolu.

Not taahhüdü aygıtını dışa açan `orchard/unstable-voting-circuits` özelliği,
üçüncü tarafların Orchard notları üzerinde devre kurabilmesi için var. Onu
kullanmak öngörülen yol, bir kenar yol değil.

## İkinci kez bakılmayı hak eden üç tasarım kararı

**Satıcı bağlaması dört alan elemanı, bir tane değil.** Yalnızca `x(pk_d)`
bağlamak bir makbuz olarak sağlam değildir: herkes, satıcının gerçek `pk_d`'sine
ama satıcının hiç kullanmadığı bir diversifier tabanına adreslenmiş bir not
üretebilir. Not düzgün taahhüt edilir, ağaca düzgün girer; ama satıcı onu ne
fark edebilir ne harcayabilir — para alınmış değil, yakılmıştır. Her iki noktanın
her iki koordinatını bağlamak bu ailenin tamamını kapatır. Maliyeti dört instance
satırı.

**Etiketler `H(alan)`'a kapsanır, istek bağlamına değil.** `context` her istekte
değişen bir nonce taşır; ondan türetilen bir nullifier de her istekte değişir ve
aynı ödemenin ikinci kez talep edilmesini yakalamakta işe yaramaz. Alana kapsamak
nullifier'ı tek doğrulayıcıda kararlı, başka her yerde ilişkisiz kılar.

**Guard soruya bakarak karar verir, yanıta asla.** İki olası yanıtın üreteceği
iki aralıktan *daha darını* değerlendirir. Fiilen alınacak dala göre karar
verseydi, yanıt beklenen yerde gelen bir ret, karşılaştırmanın hangi yöne
gittiğini sızdırırdı.

## Mevcut çalışmalardan farkı

- **[Glasspane](https://github.com/dolepee/glasspane)** doğrulayıcıya bir
  Outgoing Cipher Key vererek tek bir çıktının şifresini çözdürür — tam değeri
  geri kazandırır. Bu ifşadır. ZClaim bir predikat kanıtlar ve hiçbir şey ifşa
  etmez.
- **[ZAP1](https://github.com/frontier-compute/zap1)** uygulama olaylarının
  BLAKE2b Merkle köklerini korumalı memolara yazar. ZK yok, tutar hakkında beyan
  yok, farklı katman.
- **[Shielded Voting](https://github.com/valargroup/voting-circuits)** en yakın
  çalışma ve aynı üst akış mekanizmasını paylaşıyor. Oylama ağırlığı için toplu
  sahiplik kanıtlıyor. ZClaim üçüncü taraf doğrulayıcılar için bir *alıcıya* ve
  ödeme predikatına bağlanır, harcanmamışlık kanıtına ihtiyaç duymaz ve Inference
  Guard'ı ekler.

Tam karşılaştırma: [`docs/research.md`](docs/research.md).

## Düzen

```
crates/zclaim-core/        predikatlar, doğrulayıcı alanları, istek bağlamı
crates/zclaim-circuits/    Halo2 devresi, tanık, açık beyan
crates/zclaim-proof/       kanıtlama/doğrulama anahtarları, kanıt baytları
crates/zclaim-inference/   Inference Guard
crates/zclaim-zcash/       anchor doğrulama, ağaç tanıkları, gRPC zincir istemcisi
crates/zclaim-protocol/    Holder ve Verifier rolleri, birbirine bağlanmış
apps/quantum-cafe/         terminal demosu
apps/verifier-demo/        tarayıcı demosu
packages/verifier-sdk/     TypeScript SDK (WASM)
```

```
docs/research.md              yığın araştırması, önceki çalışmalar, fizibilite
docs/architecture-decision.md tasarım ve henüz doğru olmayanlar
docs/threat-model.md          neyin geçerli olduğu, neyin olmadığı, kime karşı
docs/demo.md                  demoyu çalıştırma ve anlatma rehberi
docs/kullanim.md              çocuğa anlatır gibi + kullanım kılavuzu
```

## Geliştirme

Rust `rust-toolchain.toml` ile sabitlenmiştir (1.97.1). `protoc` gerekmez —
protobuf mesajları elle tanımlanmıştır.

```bash
cargo test --workspace --release
cargo test --workspace --release --features zclaim-zcash/lightwalletd
cargo clippy --workspace --all-targets -- -D warnings

cargo run --release -p quantum-cafe            # demo
cargo run --release -p quantum-cafe -- --chain # ...canlı testnet'e karşı
cargo run -p zclaim-zcash --features lightwalletd --example chain

cd apps/verifier-demo && npm install && npm run dev   # tarayıcı demosu
```

İlk release derlemesi kanıtlama anahtarını üretir ve birkaç dakika sürer. Demodan
önce bir kez çalıştırın.
