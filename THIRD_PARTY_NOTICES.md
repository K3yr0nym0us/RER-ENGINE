# Third-Party Notices / Avisos de terceros

Este archivo concentra los créditos, licencias y referencias de componentes de
terceros cuyo código ha sido portado o adaptado dentro de RER-ENGINE. Los avisos
se mantienen aquí para cumplir con las obligaciones de atribución de sus
licencias; los módulos correspondientes apuntan a este documento en lugar de
repetirlos en cada archivo fuente.

---

## FastNoiseLite

- **Repositorio:** https://github.com/Auburn/FastNoiseLite
- **Archivo de referencia:** `CSharp/FastNoiseLite.cs`
- **Uso en RER-ENGINE:** puerto a Rust del algoritmo de ruido y de las tablas de
  gradientes (`src/main/Engine/worldgen/src/noise/`). Los literales de las tablas
  se generaron desde el código original para que los valores coincidan con la
  referencia.
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

## KawaiiFluid

- **Repositorio:** https://github.com/GTLBruteForce/KawaiiFluid
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
temas de features, optimización de chunks y convención de densidad) a:

- **primitive-voxels-godot** — https://github.com/TsokasGit/primitive-voxels-godot
  (sistema procedural de terreno voxel con Marching Cubes para Godot 4). El
  archivo `LICENSE` del repositorio es CC0 1.0 Universal; el README indica MIT.
- **VoxelLodTerrain / Voxel Tools (Zylann)** — https://github.com/Zylann/godot_voxel
  (MIT): hilo de biomas y estructura de chunks por capas Y.
- **Clasificación de biomas de Whittaker** (temperatura + humedad) y la
  aproximación de *Neo Terrain* (temperatura, humedad y lapse rate).

Se usaron como referencia conceptual; el código de RER-ENGINE está implementado
en Rust dentro del crate `worldgen`.

---

## Convenciones de interfaz y API (referencias conceptuales)

Patrones de interacción y de API que sirvieron de guía, sin código copiado:

- **Blender**: esquema de navegación del viewport (MMB órbita, Shift+MMB paneo,
  Ctrl+MMB dolly, rueda zoom, Numpad `.` encuadrar) y corrección de orientación
  Z-up → Y-up en la importación glTF.
- **Godot**: patrón de cuerpo cinemático (`CharacterBody`) para el deslizamiento
  multi-paso con snap al suelo y atlas de glifos para la UI del jugador.
- **Minecraft**: mirar alrededor con el botón derecho del ratón.

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
