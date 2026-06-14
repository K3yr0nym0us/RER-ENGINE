# LISTA DE TIPOS DE COMPRESION RECOMENDADA 
## (PARA LA VERSION FINAL DEL JUEGO AL EXPORTAR EL COMPILADO PARA UNA PLATAFORMA)

## MOBILES Android/iOS ASTC
Albedo     → ASTC 6x6
Normal     → ASTC 6x6
ORM        → ASTC 8x8
UI         → ASTC 4x4

## TODO LO DEMAS (Depende el tipo de textura)
✓ Albedo      → BC7
✓ Normal      → BC5
✓ ORM         → BC7
✓ Emissive    → BC7
✓ HDR         → BC6H
✓ UI          → Sin compresión
✓ Generar mipmaps para todo excepto UI
✓ Empaquetar AO+Roughness+Metallic en una sola textura


### ADICIONAL
En lugar de guardar:

Roughness
Metallic
AO

por separado, muchos motores hacen:

R = AO
G = Roughness
B = Metallic

Una sola textura llamada ORM o ARM