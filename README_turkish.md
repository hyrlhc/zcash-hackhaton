# ZClaim

**Veriyi isteme. Kanıtı iste.**

Zcash ödemeyi zaten gizliyor. Biz o gizliliği bozmadan bir uygulamanın **tek
bir soru** sorabilmesini istiyoruz.

---

## Ne yapmaya çalışıyoruz

Korumalı bir ZEC ödemesi explorer’da görünmez. Bu Zcash’in işi. Bir hizmetin
karar vermesi gerektiğinde aynı şey sorun olur:

> Bu kişi gişeye en az bilet kadar ödedi mi?

Saf Zcash bugün iki kötü seçenek sunar:

1. **Hiç görme.** Kapı kördür. “Ödedi mi?” diye bir komut yoktur.
2. **Viewing key ver.** Kapı tam tutarı, memoyu, sorulmayan her şeyi açar.
   Bir bit için mahremiyet harcanır.

Üçüncü cevap: **evet veya hayır.** Tutar değil. Cüzdan değil. İşlem değil.
Başka ödemeler değil.

Ürün bu kadar.

```
Gizli ödeme              Tek soru                 Tek bit
Cafe'ye 2,7 ZEC      →   “en az 1 ZEC mi?”    →   EVET
                          tam tutar                hâlâ gizli
```

Cafe sonra `≥ 2`, `≥ 2,5`, `≥ 2,6` diye 2,7’yi avlarsa cüzdan durur. Dürüst
soruların dizisi de saldırıdır. Buna **Inference Guard** diyoruz.

---

## Saf Zcash ne yapıyor — ZClaim ne ekliyor

| | Saf Zcash | ZClaim ile |
|---|---|---|
| Explorer | Ödeme gizli | Yine gizli |
| Uygulama “yeterince ödendi mi?” | Hayır. Kör, ya da fişi aç | Evet. Bir bit |
| Viewing key | Makbuzu açar | Kullanılmaz |
| Aynı cevap başka uygulamada | — | Geçmez. Tek isteğe kilitli |
| Eşiği yükseltip tekrar sor | — | Cüzdan reddedebilir |

Zcash gizliliğinin yerine geçmiyoruz. Onu **harcamadan** uygulamayı
çalıştırıyoruz. Kapıda zincir susar. Telefon veri değil kanıt uzatır.

Şeffaf bir zincirde bu soru boştur: tutar zaten açıktır. ZClaim ancak Zcash
ödemeyi *önce* gizlediği için anlamlıdır.

---

## Çalıştır

```bash
cargo run --release -p quantum-cafe
```

Tarayıcı:

```bash
cargo run -p chain-api --release
cd apps/verifier-demo && npm install && npm run dev
```

[http://localhost:5173](http://localhost:5173) — ZClaim kapalıyken zincirin ne
yayımladığı (blok, kök; kim ve tutar yok). Açınca soru sorulur.

Canlı kökler: `https://testnet.zec.rocks:443` (public testnet, sadece okuma).
Bu demo ZEC göndermez. Kanıttaki 2,7 ZEC notu yerelde kurulur; demo bunu
söyler. CLI’de `--chain` gerçek doğrulayıcının o yerel kökü **reddettiğini**
gösterir.

Anlatım: [`docs/kullanim.md`](docs/kullanim.md).

---

## Dürüst sınır

Kriptografi gerçek. Public testnet’te bize ait bir not henüz kanıtlayıcıya
bağlı değil — bunun için fonlu cüzdan taraması gerekir, yeni devre değil.
Aksini iddia etmiyoruz.

---

## Nasıl yaptığımız (kısa)

Pasta üzerinde Halo2 IPA (`k = 11`), Zcash’in kendi `NoteCommit^Orchard` ve
`MerkleCRH^Orchard` aygıtları (`orchard/unstable-voting-circuits`). Güvenilen
kurulum yok, satıcı imzalı credential yok.

Cüzdan şunu kanıtlar: *bu Orchard notu bu ağaç kökünün altında, bu alıcıya
gidiyor, tutar bu eşiği geçiyor* — bu uygulamaya ve bu isteğe kilitli.
Doğrulayıcı kanıtı **ve** kökün gerçek zincir kökü olduğunu kontrol eder.
Etiketler uygulamaya göredir; iki kapı log birleştiremez. Guard aralık
genişliğine bakar, tutarı iğneleyecek soruyu reddeder; kararı cevaba değil
soruya göre verir.

Test: `cargo test --workspace --release`. Ayrıntı:
[`docs/architecture-decision.md`](docs/architecture-decision.md),
[`docs/threat-model.md`](docs/threat-model.md).

Veriyi isteme. Kanıtı iste.
