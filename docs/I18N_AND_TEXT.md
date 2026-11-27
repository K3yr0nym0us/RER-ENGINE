# Internacionalización y texto del HUD

Contrato implementado para proyectos 2D/3D en la rama `feat/i18n-cosmic-text-v0`. Este documento describe el runtime del editor y del preview de juego; no describe un exportador de juego final.

## Alcance

- El manifiesto raíz del proyecto usa `i18n`, no `version` ni `language`.
- Los catálogos son JSON independientes, uno por combinación de locale y namespace.
- El texto y los botones del HUD guardan un `TextValue` (`localized` o `literal`).
- El HUD usa `cosmic-text` mediante `TextSystem`, con fuentes integradas y fallback determinista.
- Rhai y el protocolo del motor pueden cambiar el idioma de catálogos y el locale de formato.
- Las pantallas `menu` se persisten y se listan, pero **no tienen renderer runtime de menús**.
- Guardar `.save` empaqueta el proyecto; **no exporta un juego final ni un ejecutable**.

## Manifiesto raíz

La raíz de `manifest.json` no tiene campos `version` ni `language`. La configuración de internacionalización es obligatoria y tiene esta forma:

```json
{
  "type": "3D",
  "gameStyle": "first-person",
  "i18n": {
    "defaultLanguage": "es",
    "fallbackLanguage": "en",
    "catalogPaths": [
      "i18n/es-game.json",
      "i18n/en-game.json",
      "i18n/es-player-ui.json"
    ]
  }
}
```

- `defaultLanguage`: idioma inicial de los catálogos al cargar el proyecto.
- `fallbackLanguage`: idioma consultado por mensaje cuando el idioma activo no tiene una traducción.
- `catalogPaths`: rutas de los JSON, en orden de declaración. Pueden ser absolutas durante la sesión; al guardar se copian a `i18n/` y se reescriben como rutas relativas dentro del `.save`.
- Los locales se validan y normalizan como BCP-47. El valor por defecto de una configuración nueva es `en` para ambos idiomas y una lista vacía de catálogos.
- Un manifiesto que todavía contenga `version` o `language` en la raíz se rechaza al abrir o empaquetar.

El `--locale en|es` del launcher/editor es el idioma inicial de la interfaz del editor. También se envía como `set_language` al adjuntar el motor; al cargar un `.save`, `i18n.defaultLanguage` del proyecto restablece el idioma de catálogos. El selector del footer del editor cambia la interfaz ES/EN y actualiza `i18n.defaultLanguage`; un idioma de catálogo distinto de ES/EN recibido por `locale_changed` se conserva en el proyecto sin inventar una traducción de la interfaz.

## Catálogos JSON

Cada archivo describe un locale y un namespace. No existe un campo de versión de catálogo.

```json
{
  "locale": "es",
  "namespace": "game",
  "entries": [
    {
      "id": {
        "namespace": "game",
        "key": "hello",
        "context": "greeting"
      },
      "source": "Hello",
      "template": {
        "text": "Hola"
      }
    },
    {
      "id": {
        "namespace": "game",
        "key": "items"
      },
      "source": "{count} item",
      "template": {
        "pluralForms": {
          "one": "{count} elemento",
          "other": "{count} elementos"
        }
      }
    }
  ]
}
```

Reglas:

- `id.namespace` debe coincidir con `namespace` del catálogo.
- `id.context` es opcional y forma parte de la identidad del mensaje.
- `source` es el texto en idioma fuente y el fallback literal si no hay entrada.
- `template` contiene exactamente una de estas formas:
  - `text`: mensaje simple.
  - `pluralForms`: categorías ICU `zero`, `one`, `two`, `few`, `many`, `other`; debe incluir `other`.
- Los plurales usan el argumento obligatorio `{count}`.
- Los placeholders son nombres como `{name}` o `{count}`. El conjunto de placeholders de la plantilla debe coincidir con el de `source`.
- Los nombrespaces están aislados: una clave de `player_ui` no se resuelve desde `game`.
- La búsqueda prueba primero los candidatos del locale solicitado, del más específico al más general. Después consulta `fallbackLanguage`. Si tampoco hay entrada, usa `source` sin traducirlo.
- El fallback se resuelve por mensaje, no por catálogo completo. El orden de `catalogPaths` desempata catálogos del mismo locale y namespace.

## Texto del HUD: `TextValue`

Los campos `text` de cajas de texto y botones usan uno de estos dos valores:

```json
{
  "kind": "localized",
  "id": {
    "namespace": "player_ui",
    "key": "title"
  },
  "source": "Título",
  "arguments": {
    "name": "Ada"
  }
}
```

```json
{
  "kind": "literal",
  "value": "Jugar"
}
```

- `localized` referencia un catálogo y conserva siempre el texto fuente.
- `literal` se muestra tal cual y no depende de catálogos.
- `arguments` es opcional y contiene valores para placeholders.
- El editor muestra `source` como etiqueta del elemento. Los textos localizados se conservan al editar propiedades; una cadena escrita en el editor se envuelve como `literal`.
- La edición directa de texto en un campo de entrada solo modifica literales. Un `localized` se mantiene intacto.
- Las listas y actualizaciones IPC del HUD serializan `TextValue`; no hay campos de texto plano legacy en el contrato runtime.

Ver [Player_UI_Model.yaml](./yaml/editor/Player_UI_Model.yaml).

## `cosmic-text` y `TextSystem`

El texto runtime del HUD se conforma y rasteriza con `cosmic-text` a través de `TextSystem`. El sistema carga seis caras embebidas y no consulta directorios de fuentes del sistema operativo:

- Inter Regular e Inter Bold, familia canónica `Inter 18pt`.
- Noto Sans Arabic.
- Noto Sans Hebrew.
- Noto Sans SC.
- Fira Mono.

Una fuente de proyecto registrada desde Recursos actúa como primaria. Si no cubre un glifo, el fallback integrado selecciona por script: Inter como base, Noto Arabic/Hebrew para sus scripts, Noto Sans SC para Han y Fira Mono para monospace. Los literales usan locale de shaping `und` para que el fallback se elija por contenido; los localizados usan el idioma activo.

Cambiar el locale de shaping conserva la base de fuentes y los IDs de las caras, y descarta las cachés de shaping/rasterizado para evitar claves obsoletas.

## Cambio de idioma y locale de formato

El runtime mantiene dos estados independientes:

- `activeLanguage`: idioma usado para resolver catálogos y dar forma al texto localizado del HUD.
- `formattingLocale`: locale de formato expuesto al host y a Rhai. No sustituye al idioma de catálogos.

Un cambio efectivo de cualquiera de los dos:

1. Actualiza el runtime y el estado Rhai compartido.
2. Invalida/reconstruye el overlay del HUD; en 3D también limpia la caché del prompt de interacción.
3. Emite `locale_changed` con la época del estado.

Asignar el mismo valor normalizado es un no-op y no emite el evento. Un locale inválido se ignora con aviso.

## Flujo de fuentes del editor

El flujo implementado es:

1. **Recursos → Fuentes**: importar o seleccionar una fuente del proyecto.
2. Abrir su **detalle**: se muestra la fuente primaria, el fallback integrado y muestras multilengüe.
3. Pulsar **Usar en HUD**.
4. El editor abre el selector nativo de texto del HUD con esa fuente preseleccionada.
5. El botón **Añadir** del selector crea el texto; `Usar en HUD` no crea el elemento por sí solo.

`Usar en HUD` requiere una pantalla HUD de jugador. Si no hay ninguna, el detalle indica que primero hay que crearla en Interfaz de usuario. La fuente se referencia por su ruta de biblioteca; no se duplica dentro de `playerUi*`.

## API Rhai

Disponible en los motores 2D y 3D:

| API | Efecto |
|-----|--------|
| `engine.set_language("es")` | Cambia el idioma de catálogos y shaping. |
| `engine.set_locale("es-MX")` | Cambia el locale de formato independiente. |
| `engine.get_language()` | Devuelve `activeLanguage` normalizado. |
| `engine.get_locale()` | Devuelve `formattingLocale` normalizado. |
| `tr(key)` / `tr(key, source)` | Busca en el namespace `game`. |
| `tr_n(key, count)` / `tr_n(key, count, source)` | Selecciona una forma plural con `{count}`. |
| `tr_fmt(key, args)` / `tr_fmt(key, args, source)` | Sustituye placeholders nombrados desde un mapa Rhai. |

Sin `source`, `tr` obtiene el texto fuente de los catálogos `game` de fallback/default; si no existe, devuelve una cadena vacía y registra un aviso. Con `source`, ese texto es el fallback literal. `tr_fmt` hace interpolación de placeholders, no parseo de ICU MessageFormat. Las reglas de plural usan el locale del catálogo que resolvió el mensaje, no `formattingLocale`.

```rhai
fn on_start(entity) {
    engine.set_language("es");
    engine.set_locale("es-MX");

    engine.log(engine.get_language() + " / " + engine.get_locale());
    engine.log(tr("hello", "Hello"));
    engine.log(tr_n("items", 2, "{count} item"));
    engine.log(tr_fmt("welcome", #{ name: "Ada" }, "Hello {name}"));
}
```

Ver [RHAI_API.yaml](./yaml/scripting/RHAI_API.yaml).

## IPC del motor

Los comandos comunes se serializan en snake_case:

```json
{ "cmd": "set_language", "language": "es" }
{ "cmd": "set_formatting_locale", "locale": "es-MX" }
```

Los eventos de carga `project_loaded_2d` y `project_loaded_3d` incluyen `activeLanguage` y `formattingLocale`. Cuando cambia el estado runtime se emite:

```json
{
  "event": "locale_changed",
  "activeLanguage": "es",
  "formattingLocale": "es-MX",
  "epoch": 7
}
```

No hay eventos independientes llamados `activeLanguage` o `formattingLocale`: son campos de los eventos de proyecto y de `locale_changed`. El launcher no envía estos comandos por stdin al motor; el editor Rust los despacha en proceso. Ver [IPC_Protocol.yaml](./yaml/engine/IPC_Protocol.yaml).

## Persistencia y límites

- Los catálogos referenciados se copian una vez al `.save`, bajo `i18n/`, con nombres seguros y únicos.
- `resources.models[].importer_version` pertenece al formato binario `.rerasset`; no es una versión del manifiesto ni del proyecto.
- El manifiesto raíz no debe recuperar `version` o `language` para representar la versión del proyecto.
- El renderer runtime actual cubre HUD y prompts de interacción. Las pantallas de menú siguen sin renderer en play.
- La exportación/cocción de un juego final y los perfiles de textura para build siguen fuera de este contrato.

## Implementación de referencia

- `src/main/Engine/core/src/i18n/`: config, catálogos, lookup, formato y `TextValue`.
- `src/main/Engine/core/src/scripting/i18n.rs` y `scripting/api.rs`: estado y API Rhai.
- `src/main/Engine/engine/src/i18n.rs`: runtime, fuentes de proyecto y resolución.
- `src/main/Engine/engine/src/text/`: `cosmic-text`, `TextSystem` y fuentes embebidas.
- `src/main/Engine/engine/src/editor/save/`: manifiesto, validación y empaquetado de catálogos.
- `src/main/Engine/engine/src/editor/ui/font_details_modal.rs`: detalle de fuente y `Usar en HUD`.
