# Chybějící výpočty pro přesnější depozici kapaliny

Přehled fyzikálních a geometrických rovnic, které v kódu DPI zatím nejsou (nebo jsou jen v zjednodušené, empirické podobě). Ke každé rovnici je popis, parametry, místo v kódu, kde by se použila, a stav ověření.

Legenda stavu:
- **Ověřeno odvozením**: rovnice je odvozena z principu (zachování hmoty, geometrie).
- **Nutno ověřit**: rovnice je z paměti nebo jde o hrubý odhad, před použitím zkontrolovat v literatuře.

---

## 0. Již částečně v kódu: průtok pístem

**Název:** Kontinuita průtoku pístu stříkačky
**Popis:** Objemový průtok stříkačkou je plocha pístu krát rychlost pístu. Dnes je to nahrazeno empirickou konstantou `calibration_factor`.
**Rovnice:**

    Q = A_p · v_p,    A_p = π · D_p² / 4

kde `Q` je objemový průtok [mm³/s = µl/s], `A_p` plocha pístu [mm²], `D_p` průměr pístu [mm], `v_p` rychlost pístu [mm/s] (z kroků krokového motoru).

**Vztah k současnému kódu:** `calibration_factor = 1 / A_p` (mm na µl), výchozí `0.323877` odpovídá `D_p ≈ 1.98 mm`.
**Místo v kódu:** `dpi-core/src/extrusion.rs:35` (`calculate_e_per_mm`).
**Stav:** Ověřeno odvozením. Zbývá ověřit, zda `D_p` skutečné stříkačky odpovídá nalezené hodnotě.
**Priorita:** Vysoká (základ všech dalších výpočtů).

---

## 1. Šířka stopy z průměru trysky a die swell

**Název:** Šířka stopy s nabobtnáním (die swell)
**Popis:** Stopa po výstupu z trysky je širší než průměr trysky, protože kapalina se po výstupu roztáhne. Faktor `B` závisí na reologii a geometrii trysky.
**Rovnice:**

    w = B · D

kde `w` je šířka stopy [mm], `D` vnitřní průměr trysky [mm], `B` faktor nabobtnání [-].

**Parametry:** `D` (známe), `B` (odhad 1,1–1,3, upřesnit měřením).
**Platnost:** Jen pro **pasty a gely**, které po výstupu z trysky drží tvar. Řídká kapalina na podložce smáčí a roztéká se, takže její šířku určuje objem na jednotku délky a kontaktní úhel, ne průměr trysky. Pro ni použít rovnici 2b.
**Místo v kódu:** nová funkce vedle `ExtrusionCalculator` (`dpi-core/src/extrusion.rs`), případně modul pro geometrii stopy.
**Stav:** Ověřeno jako tvar vztahu. Hodnota `B` je odhad, nutno změřit.
**Priorita:** Vysoká (pro pasty).

---

## 2. Výška stopy z zachování hmoty

**Název:** Výška stopy z kontinuity
**Popis:** Objem, který vyteče za jednotku času, musí odpovídat objemu nanesené stopy. Z průtoku a šířky se dopočítá výška.
**Rovnice:**

    Q = w · h · v      (obdélníková aproximace)
    h = Q / (w · v)

nebo pro eliptický průřez:

    Q = (π/4) · w · h · v
    h = 4Q / (π · w · v)

kde `h` je výška stopy [mm], `v` rychlost trysky [mm/s], `Q` průtok [mm³/s].

**Parametry:** `Q` (z rovnice 0), `w` (z rovnice 1), `v` (známe).
**Platnost:** Stejně jako rovnice 1 jen pro pasty a gely. Eliptický průřez odpovídá půlelipse s poloosami `w/2` a `h`, jejíž plocha je `½·π·(w/2)·h = π·w·h/4`.
**Místo v kódu:** stejný modul jako rovnice 1.
**Stav:** Ověřeno odvozením. Volba průřezu (obdélník nebo elipsa) je modelová, lze ji ověřit mikroskopem.
**Priorita:** Vysoká (pro pasty).

---

## 2b. Průřez stopy smáčivé kapaliny (kruhová úseč)

**Název:** Stopa kapaliny jako válcová úseč
**Popis:** Řídká kapalina se na podložce roztéká, dokud nedosáhne kontaktního úhlu `θ`. Průřez čáry je pak kruhová úseč, tedy 2D obdoba kulové úseče z rovnice 6. Plocha průřezu se rovná objemu na jednotku délky `Q/v`.
**Rovnice:**

    Q / v = (w/2)² · (θ − sin θ · cos θ) / sin² θ
    h = (w/2) · tg(θ/2)

kde `w` je šířka stopy [mm], `h` výška stopy [mm], `θ` kontaktní úhel [rad], `Q` průtok [mm³/s], `v` rychlost trysky [mm/s].

**Odvození:** kruhová úseč o poloměru `r` s poloviční šířkou `w/2 = r·sin θ` má plochu `r²·(θ − sin θ·cos θ)`. Kontrola: pro `θ = 90°` vychází půlkruh `π·(w/2)²/2`.
**Parametry:** `Q` (z rovnice 0), `v` (známe), `θ` (změřit nebo zadat podle podložky). Z první rovnice se spočítá `w`, z druhé `h`.
**Platnost:** Pro kapaliny, které podložku smáčí, a pro šířku menší než kapilární délku (viz rovnice 6). Nepočítá s nestabilitou čáry: příliš široká nebo příliš rychle nanášená čára se rozpadá na kapky.
**Místo v kódu:** stejný modul jako rovnice 1 a 2. Volba mezi 1+2 (pasta) a 2b (kapalina) podle typu materiálu.
**Stav:** Ověřeno odvozením.
**Priorita:** Vysoká (pro kapaliny).

---

## 3. Průtok tryskou pro newtonskou kapalinu (Hagen-Poiseuille)

**Název:** Hagen-Poiseuilleův zákon
**Popis:** Laminární průtok newtonskou kapalinou trubicí. Dává vztah mezi tlakem a průtokem. Potřebné jen pokud chceme předpovídat tlak nebo kontrolovat, zda krokový motor kapalinu vytlačí.
**Rovnice:**

    Q = π · R⁴ · ΔP / (8 · μ · L)

kde `R = D/2` poloměr kapiláry [m], `ΔP` tlakový spád [Pa], `μ` dynamická viskozita [Pa·s], `L` délka kapiláry [m].

**Parametry:** `D`, `L` (známe nebo odhad), `μ` (volitelné, z viskozimetru).
**Místo v kódu:** pomocný výpočet, např. kontrola síly motoru. Není nutný pro průtokově řízený model.
**Stav:** Ověřeno, klasická rovnice.
**Priorita:** Nízká (jen pro kontrolu tlaku).

---

## 4. Průtok tryskou pro power-law kapalinu

**Název:** Průtok power-law kapalinou trubicí
**Popis:** Zobecnění Hagen-Poiseuille pro kapaliny se smykovým ztenčováním nebo zahušťováním (`τ = K·γ̇ⁿ`). Pro `n = 1` přechází na Hagen-Poiseuille.
**Rovnice:**

    Q = (π · n / (3n + 1)) · (ΔP / (2 · K · L))^(1/n) · R^((3n+1)/n)

kde `K` konzistenční index [Pa·sⁿ], `n` index toku [-], ostatní jako v rovnici 3.

**Parametry:** `K`, `n` (z reometru, volitelné), `D`, `L`.
**Místo v kódu:** stejně jako rovnice 3, jen pokud se měří reologie.
**Stav:** Ověřeno jako standardní vztah (Bird-Stewart-Lightfoot). Před použitím zkontrolovat v literatuře.
**Priorita:** Nízká (volitelné).

---

## 5. Minimální tlak pro tok kapalinou s mezí toku (Herschel-Bulkley / Bingham)

**Název:** Prahový tlak pro start toku
**Popis:** Kapaliny s mezí toku (pasty, gely) začnou téct až při překročení napětí `τ₀` na stěně. Pod tímto tlakem je průtok nulový.
**Rovnice:**

    ΔP_min = 2 · L · τ₀ / R

kde `τ₀` mez toku [Pa].

**Parametry:** `τ₀` (z reometru), `L`, `R`.
**Místo v kódu:** kontrola, zda krokový motor překoná start. Relevantní jen pro pasty a gely.
**Stav:** Ověřeno odvozením z rovnováhy napětí ve válci. Hodnota `τ₀` z měření.
**Priorita:** Nízká (volitelné, záleží na kapalině).

---

## 6. Objem kulové úseče (kapka nebo stopa na podložce)

**Název:** Objem kulové úseče
**Popis:** Tvar kapky na rovné podložce při kontaktním úhlu `θ` odpovídá kulové úseči. Ze známého objemu a výšky lze dopočítat poloměr stopy.
**Rovnice:**

    V = π · h · (3a² + h²) / 6

kde `V` objem [mm³], `h` výška kapky [mm], `a` poloměr základny [mm]. Geometrický vztah výšky a základny pro kontaktní úhel `θ` (kulová úseč, `a = R·sin θ`, `h = R·(1 − cos θ)`) je:

    h = a · tg(θ/2)

**Parametry:** `V` (z průtoku a času), `θ` (odhad podle podložky), `h` nebo `a` (dopočítá se).
**Platnost:** Jen pro kapky menší než kapilární délka `l_c = √(σ/(ρ·g))`, u vody asi 2,7 mm. Větší kapky gravitace zploští a tvar už není kulová úseč.
**Místo v kódu:** výpočet velikosti tečky nebo stopy na podložce.
**Stav:** Ověřeno odvozením, včetně `h = a·tg(θ/2)` (z `a = R·sin θ` a `h = R·(1 − cos θ)` plyne `h/a = (1 − cos θ)/sin θ = tg(θ/2)`).
**Priorita:** Střední.

---

## 7. Odpařování kapky nebo stopy (difuzní model)

**Název:** Úbytek objemu odpařováním
**Popis:** Rozpouštědlo se odpařuje difuzí páry od povrchu kapky do okolního vzduchu. Rychlost závisí na velikosti kapky, kontaktním úhlu, koncentraci nasycené páry a vlhkosti vzduchu. Ovlivní objem, kontaktní úhel a koncentraci rozpuštěné látky v čase.
**Rovnice (aproximace Hu & Larson, 2002):**

    dV/dt = −(π · R · D_v · c_sat · (1 − RH) / ρ_l) · (0,27 · θ² + 1,30)

kde `V` objem [m³], `t` čas [s], `R` poloměr základny [m], `D_v` difuzní koeficient páry ve vzduchu [m²/s], `c_sat` koncentrace nasycené páry [kg/m³], `RH` relativní vlhkost [-], `ρ_l` hustota kapaliny [kg/m³], `θ` kontaktní úhel [rad]. Vše v jednotkách SI. Výsledek v m³/s se pro mm³/s násobí 10⁹.

**Oprava oproti původní verzi:** původní tvar `dV/dt = −4·D_v·c_sat·R·f(θ)·(1 − RH)`:
- **Chybějící hustota:** výraz dává kg/s, tedy úbytek hmotnosti, ne objemu. Pro objem je potřeba dělit hustotou `ρ_l`.
- **Faktor 4:** platí jen pro funkci `f(θ)` normovanou na `f(0) = 1`. Vychází z Popovova modelu `dm/dt = −π·R·D_v·Δc·f_P(θ)`, kde `f_P(0) = 4/π`.
- **Nová podoba:** Hu & Larson nahrazují `f(θ)` explicitním polynomem. Pro `θ → 0` dává `π·1,30 ≈ 4,08`, což odpovídá Deeganově limitě 4.
- **Vlhkost:** člen `(1 − RH)` je správně, protože `c_sat − c_∞ = c_sat·(1 − RH)`.

**Parametry:** `D_v`, `c_sat` (závisí na látce a teplotě, tabelované), `ρ_l`, `RH`, `θ`, `R`.
**Platnost:**
- **Předpoklady:** kapka s ukotveným okrajem, `θ < 90°`, difuzí řízené odpařování v klidném vzduchu.
- **Teplota:** `c_sat` brát při teplotě povrchu kapky. Na vyhřívané podložce je výrazně vyšší než při teplotě vzduchu.
- **Čára:** pro dlouhou čáru jen hrubý odhad, protože jiná geometrie dává jinou rychlost odpařování.
- **Proudění vzduchu:** ventilátor nebo průvan odpařování zrychlí.

**Místo v kódu:** nová funkce, která by upravila objem stopy v čase; dále by ovlivnila G-kód (např. pauzu mezi nanášením).
**Stav:** Tvar rovnice ověřen (Hu & Larson 2002, Popov 2005, Deegan et al. 2000). Hodnoty konstant je třeba dohledat pro konkrétní rozpouštědlo.
**Priorita:** Vysoká pro těkavé kapaliny (kapalina se po nanesení vypařuje).

---

## 8. Youngova rovnice (kontaktní úhel)

**Název:** Youngova rovnice
**Popis:** Rovnováha povrchových napětí na tří-fázovém rozhraní určuje kontaktní úhel kapky na podložce. Používá se hlavně k odhadu `θ` z naměřených napětí, ne k přímému výpočtu v kódu.
**Rovnice:**

    σ_SV = σ_SL + σ · cos θ

kde `σ_SV` napětí pevná látka–pára, `σ_SL` napětí pevná látka–kapalina, `σ` povrchové napětí kapaliny, `θ` kontaktní úhel.

**Parametry:** `σ` (volitelně změřeno), `σ_SV`, `σ_SL` (vlastnosti podložky, neznámé).
**Místo v kódu:** pouze jako odhad `θ` do rovnice 6 a 7, nebo jako vstup uživatele.
**Stav:** Ověřeno, klasická rovnice. Hodnoty `σ_SV`, `σ_SL` z literatury nebo měření.
**Priorita:** Nízká.

---

## 9. Kapilární číslo (poměr viskózních a kapilárních sil)

**Název:** Kapilární číslo
**Popis:** Určuje, zda převládají viskózní síly nebo povrchové napětí. Pomáhá odhadnout, jak se stopa rozlévá nebo tvoří kapky při pohybu trysky.
**Rovnice:**

    Ca = μ · v / σ

kde `μ` dynamická viskozita [Pa·s], `v` rychlost trysky [m/s], `σ` povrchové napětí [N/m].

**Parametry:** `μ`, `σ` (volitelné, z měření), `v` (známe).
**Místo v kódu:** diagnostika nebo varování při nevhodných parametrech.
**Stav:** Ověřeno odvozením z definice.
**Priorita:** Nízká (volitelné).

---

## 10. Ohnesorgeovo číslo a tisknutelnost kapek

**Název:** Ohnesorgeovo číslo
**Popis:** Určuje, zda z trysky vznikne stabilní kapka. Relevantní pouze pro piezo nebo drop-on-demand dávkování. **Pro dávkování přes stříkačku se nepoužije**, zachováno jen jako referenční.
**Rovnice:**

    Oh = μ / √(ρ · σ · D)
    Z = 1 / Oh

Tisknutelné okno (Reis & Derby, 2000): `1 < Z < 10`.

kde `ρ` hustota [kg/m³], `σ` povrchové napětí [N/m], `D` průměr trysky [m].

**Parametry:** `ρ`, `σ`, `μ` (volitelné), `D`.
**Místo v kódu:** nepoužije se pro stříkačku.
**Stav:** Vzorec ověřen v literatuře. Hranice okna nutno ověřit pro konkrétní použití.
**Priorita:** Nízká (pouze pro drop-on-demand).

---

## Shrnutí priorit

| Priorita | Rovnice | Stav ověření |
|---|---|---|
| Vysoká | 0. Kontinuita pístu (částečně v kódu) | Odvozeno |
| Vysoká | 1. Šířka stopy s die swell (pasty) | Odvozeno, B odhad |
| Vysoká | 2. Výška stopy z kontinuity (pasty) | Odvozeno |
| Vysoká | 2b. Průřez stopy kapaliny (kruhová úseč) | Odvozeno |
| Vysoká | 7. Odpařování (Hu & Larson) | Opraveno, ověřeno v literatuře |
| Střední | 6. Objem kulové úseče | Odvozeno |
| Nízká | 3. Hagen-Poiseuille | Odvozeno |
| Nízká | 4. Power-law průtok | Ověřit v literatuře |
| Nízká | 5. Prahový tlak | Odvozeno |
| Nízká | 8. Youngova rovnice | Odvozeno |
| Nízká | 9. Kapilární číslo | Odvozeno |
| Nízká | 10. Ohnesorge | Pouze drop-on-demand |
