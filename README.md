# ZClaim

## Veriyi isteme. Kanıtı iste.

Zcash bir ödemenin tutarını, taraflarını ve işlem ayrıntılarını gizler. Bu güçlü bir mahremiyet sağlar; ancak bir uygulamanın basit bir koşulu doğrulaması gerektiğinde iki kötü seçenek bırakır: hiçbir şey öğrenememek veya kullanıcıdan gereğinden fazla bilgi istemek.

ZClaim bu ikisinin arasına üçüncü bir yol koyar.

Bir uygulama “Bu kullanıcı en az 5 ZEC ödedi mi?” diye sorar. Kullanıcının cüzdanı ödeme kaydını paylaşmak yerine yalnızca koşulun doğru olduğunu gösteren bir kanıt üretir. Uygulama sonucu doğrular; tam tutarı, cüzdan adresini, işlem kimliğini veya diğer ödemeleri öğrenmez.

```text
Gizli Zcash ödemesi  →  “Ödeme en az 5 ZEC mi?”  →  TRUE
                                  |
                    tutar ve işlem ayrıntıları gizli kalır
```

## Neden Zcash?

ZClaim, Zcash’in sunduğu gizliliğin yerine geçmez. Onun üzerine çalışır.

Zcash’in shielded ödemeleri tutarı ve tarafları zincirde açıkça yayınlamaz. Buna rağmen her ödeme, Zcash’in korumalı durumunun parçasıdır. ZClaim, cüzdanın bildiği ödeme bilgisini bu zincir durumuna bağlar ve yalnızca istenen koşulu kanıtlar.

Bu nedenle uygulama bir viewing key istemez, ödeme fişini açmaz ve kullanıcıdan zincir geçmişini paylaşmasını beklemez. Zcash ödemeyi gizli tutar; ZClaim uygulamanın ihtiyaç duyduğu sınırlı cevabı üretir.

## Bir cevap neden yeterli değil?

Tek bir `TRUE` veya `FALSE` cevabı az bilgi taşır. Fakat aynı uygulama eşikleri değiştirerek tekrar tekrar sorarsa gizli tutara yaklaşabilir:

```text
En az 1 ZEC mi?    TRUE
En az 2 ZEC mi?    TRUE
En az 2,5 ZEC mi?  TRUE
En az 2,6 ZEC mi?  ...
```

Sorular tek başlarına makul görünse de birlikte bir indeksleme saldırısına dönüşür. ZClaim bu yüzden yalnızca kanıt üretmez; uygulamanın zaman içinde ne kadar bilgi öğrendiğini de takip eder.

Inference Guard, yeni bir sorunun olası iki cevabını da önceden değerlendirir. Soru gizli tutarı izin verilen aralıktan fazla daraltabilecekse cüzdan cevap üretmez. Karar gerçek cevaba göre verilmez; böylece reddedilmenin kendisi de ek bilgi sızdırmaz.

Guard uygulamada değil cüzdanda çalışır. Çünkü sorguyu yapan tarafın kendi erişimini sınırlamasına güvenilemez. Mahremiyetin kontrolü, verinin sahibi olan tarafta kalmalıdır.

## Ne doğrulanır, ne gizli kalır?

Uygulama şunları doğrulayabilir:

- Ödemenin belirtilen koşulu sağladığını
- Ödemenin doğru alıcıyla ilişkili olduğunu
- Kanıtın belirli bir uygulama ve istek için üretildiğini
- Aynı ödemenin aynı uygulamada tekrar kullanılmadığını
- Ödemenin kabul edilen bir Zcash durumuna bağlı olduğunu

Şunları öğrenmez:

- Tam ödeme tutarını
- Kullanıcının cüzdan adresini
- İşlem kimliğini
- Memo içeriğini
- Kullanıcının diğer ödemelerini
- Aynı kullanıcının başka uygulamalardaki hareketlerini

Kanıtlar uygulamaya özel bağlanır. Bir hizmet için üretilen cevap başka bir hizmette kullanılamaz; farklı uygulamalar kayıtlarını birleştirerek ortak bir kullanıcı profili çıkaramaz.

## Projenin bugünkü sınırı

Kanıt sistemi Zcash’in Orchard yapısını ve gerçek sıfır bilgi kanıtlarını kullanır. Uygulama ayrıca public Zcash testnet durumunu okuyabilir ve doğrulayıcının kabul ettiği zincir köklerini takip edebilir.

Ancak mevcut demo ödemesi henüz public testnet üzerinde oluşturulmuş bir nota bağlı değildir. Demo, yerelde hazırlanmış gerçek bir Orchard ödeme tanığıyla çalışır. Bu nedenle kriptografik kanıt gerçektir; demo ödemesinin public zincir geçmişi değildir. Arayüz bu iki kökü ayrı gösterir ve zincire ait olmayan demo kökünü gerçek testnet kökü gibi sunmaz.

Inference Guard da tek başına mutlak bir güvenlik garantisi değildir. Sorgu geçmişinin kalıcı tutulması ve cüzdan tarafından zorunlu uygulanması gerekir. Geçmiş silinirse veya kullanıcı korumasız bir cüzdan kullanırsa sorgu sınırı aşılabilir.

## Amaç

ZClaim’in amacı gizli bir ödemeyi görünür hale getirmek değil, uygulamaların o ödemeyle ilgili en az bilgiyle karar verebilmesini sağlamaktır.

Bir üyelik sistemi, bilet kapısı, sadakat programı veya ödeme API’si bütün fişi istememelidir. İhtiyacı yalnızca bir koşulun doğru olup olmadığını bilmekse, yalnızca bunun kanıtını almalıdır.

**Veriyi isteme. Kanıtı iste.**

Teknik tasarım ve güvenlik sınırları için [mimari kararı](docs/architecture-decision.md) ve [tehdit modelini](docs/threat-model.md) inceleyebilirsiniz.
