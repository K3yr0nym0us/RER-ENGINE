# Third-Party Notices / Avisos de terceros

Este archivo concentra los créditos, licencias y referencias de componentes de
terceros cuyo código ha sido portado o adaptado dentro de RER-ENGINE. Los avisos
se mantienen aquí para cumplir con las obligaciones de atribución de sus
licencias; los módulos correspondientes apuntan a este documento en lugar de
repetirlos en cada archivo fuente.

---

## Ruido procedimental — tablas de gradientes

- **Uso en RER-ENGINE:** puerto a Rust del algoritmo de ruido y de las tablas de
  gradientes (`src/main/Engine/worldgen/src/noise/`). Los literales de las tablas
  se generaron a partir de la implementación de referencia para que los valores
  coincidan bit a bit.
- **Licencia:** MIT — Copyright (c) 2020 Jordan Peck (jordan.me2@gmail.com);
  Copyright (c) 2020 Contributors.

> MIT License
>
> Copyright(c) 2020 Jordan Peck (jordan.me2@gmail.com)
> Copyright(c) 2020 Contributors
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

---

## Núcleo físico de fluidos

- **Uso en RER-ENGINE:** puerto a Rust del núcleo físico del sistema de fluidos,
  analizado shader a shader
  (`src/main/Engine/engine/src/engine3d/particles/fluid.rs`).
- **Licencia:** MIT — Copyright (c) 2026 Team_Bruteforce.

> MIT License
>
> Copyright (c) 2026 Team_Bruteforce
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

---

## Tablas de Marching Cubes

- **Referencias:** Paul Bourke (1994), *Polygonising a scalar field*; tabla de
  aristas/corners de Cory Gene Bloyd.
- **Uso en RER-ENGINE:** tablas canónicas de Marching Cubes en
  `src/main/Engine/worldgen/src/mesher/tables.rs`. Los literales se transcriben
  de las tablas de referencia y conservan dígitos que exceden la mantisa de f32
  (warning cosmético).

---

## Referencias de diseño (worldgen)

El generador de mundos toma como guía de diseño (patrones de seeds por dominio,
temas de features, optimización de chunks y convención de densidad) referencias
conceptuales de:

- Sistemas procedurales de terreno voxel con Marching Cubes.
- Hilos de biomas y estructura de chunks por capas Y.
- Clasificación de biomas por temperatura y humedad, con aproximación de lapse
  rate.

Se usaron como referencia conceptual; el código de RER-ENGINE está implementado
en Rust dentro del crate `worldgen`.

---

## Convenciones de interfaz y API (referencias conceptuales)

Patrones de interacción y de API que sirvieron de guía, sin código copiado:

- Esquema de navegación de viewport estilo DCC (MMB órbita, Shift+MMB paneo,
  Ctrl+MMB dolly, rueda zoom, Numpad `.` encuadrar) y corrección de orientación
  Z-up → Y-up en la importación glTF.
- Patrón de cuerpo cinemático para el deslizamiento multi-paso con snap al suelo
  y atlas de glifos para la UI del jugador.
- Mirar alrededor con el botón derecho del ratón.

---

## Referencias académicas (solver de fluidos)

Las ecuaciones del solver de fluidos
(`src/main/Engine/engine/src/engine3d/particles/fluid.rs`) se basan en:

- M. Macklin y M. Müller, "Position Based Fluids", *ACM Transactions on
  Graphics* 32(4), 2013 — λ, Δp y término de corrección de superficie s_corr
  (Ec. 13).
- M. Macklin et al., "Unified Particle Physics for Real-Time Applications",
  *ACM Transactions on Graphics* 33(4), 2016 — compliance `alpha/dt²` en el
  constraint de densidad (XPBD).
- N. Akinci et al., "Versatile Surface Tension and Adhesion for SPH Fluids",
  *ACM Transactions on Graphics* 32(6), 2013 — cohesión con spline C(r)
  (atracción a r≈h/2, repulsión de cerca).
- N. Akinci et al., "Versatile Rigid-Fluid Coupling for Incompressible SPH",
  *IEEE Transactions on Visualization and Computer Graphics* 18(12), 2012 —
  partículas de frontera: lattice virtual fuera de las paredes con masa
  efectiva ψ que reconstruye la media cáscara de vecinos que las paredes quitan.
- Viscosidad XSPH (Macklin & Müller, 2013) — difusión de velocidad entre vecinos.

---

## rstar (vendor local)

- **Uso en RER-ENGINE:** `src/main/Engine/vendor/rstar/` contiene una copia de
  `rstar 0.13.1` (crate usado por `parry3d`/`rapier3d`) con una unica
  modificacion: la dependencia `heapless` pasa de `0.8` a `0.9` para eliminar
  los avisos RUSTSEC-2026-0574..0582 (unsound). La API que rstar utiliza
  (`binary_heap::BinaryHeap::new/push/pop`) es compatible entre ambas versiones.
  Se aplica mediante `[patch.crates-io]` en `src/main/Engine/Cargo.toml`.
- **Licencia original:** MIT OR Apache-2.0 - Copyright (c) Stefan Altmayer y
  los contribuidores de geo-rust. Se conservan `Cargo.toml`, `Cargo.toml.orig`,
  `README.md` y `CHANGELOG.md` del crate original. Textos completos de la
  licencia: https://github.com/georust/rstar
- **Retirada:** cuando `parry3d` pase a usar `rstar >= 0.14` (que ya depende de
  `heapless 0.9`), eliminar el directorio `vendor/rstar` y la entrada `[patch.crates-io]`.

---

## Inter (fuentes del editor y del HUD)

- **Uso en RER-ENGINE:** `src/main/Engine/assets/fonts/` contiene
  `Inter_18pt-Regular.ttf` e `Inter_18pt-Bold.ttf`, enlazadas dentro del binario
  del motor con `include_bytes!` (`src/main/Engine/engine/src/embedded_fonts.rs`).
  Son las fuentes fijas de la UI egui y del HUD del jugador; no se leen fuentes
  del sistema operativo. A los TTF embebidos se les han retirado los mapeos del
  Área de Uso Privado (U+E000..U+F8FF) para que no sustituyan a los iconos
  Phosphor; el resto de la fuente es la Inter original.
- **Licencia:** SIL Open Font License 1.1 — Copyright 2020 The Inter Project
  Authors (https://github.com/rsms/inter). El texto completo se conserva en
  `src/main/Engine/assets/fonts/OFL.txt`.

---

## Fuentes de fallback deterministas

- **Uso en RER-ENGINE:** fuentes empaquetadas sin modificar en
  `src/main/Engine/assets/fonts/` para el fallback determinista de texto árabe,
  hebreo, chino simplificado y monoespaciado, sin depender de fuentes del
  sistema operativo.
- **Origen de Noto Sans Arabic, Noto Sans Hebrew y Fira Mono:** copias de los
  archivos incluidos en el directorio `fonts/` del crate `cosmic-text 0.19.0`
  (https://crates.io/crates/cosmic-text/0.19.0). Las fuentes Noto proceden del
  proyecto Google Noto; Fira Mono fue publicada originalmente por Mozilla
  Foundation y Telefonica S.A.
- **Noto Sans Arabic y Noto Sans Hebrew:** `NotoSansArabic.ttf` y
  `NotoSansHebrew.ttf`. **Licencia:** SIL Open Font License 1.1 — Copyright 2012
  Google Inc. All Rights Reserved. El texto completo se conserva en
  `src/main/Engine/assets/fonts/NotoSansArabic-Hebrew-LICENSE.txt`.
- **Fira Mono:** `FiraMono-Medium.ttf`. **Licencia:** SIL Open Font License 1.1 —
  Digitized data copyright (c) 2012-2015, The Mozilla Foundation and Telefonica
  S.A. El texto completo se conserva en
  `src/main/Engine/assets/fonts/FiraMono-LICENSE.txt`.
- **Origen de Noto Sans SC:** descargado del repositorio Google Fonts:
  `https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf`.
  Se distribuye localmente como `NotoSansSC-Variable.ttf`, junto con su licencia
  `NotoSansSC-OFL.txt`, obtenida de
  `https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/OFL.txt`.
- **Licencia de Noto Sans SC:** SIL Open Font License 1.1 — Copyright 2014-2021
  Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'.
