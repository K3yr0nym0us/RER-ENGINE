const UI_LABELS_ES = [
  'Propiedades de la entidad',
  'Convertir en Blueprint',
  'Construcción rápida',
  'Programar entidad',
  'Crear nueva escena',
  'Crear entidad',
  'Interfaz de usuario',
  'Scene script',
  'Scene logic',
  'Herramientas',
  'Controles',
  'Recursos',
  'Escenas',
  'viewport',
  'Guardar',
]

const UI_LABELS_EN = [
  'Entity properties',
  'Convert to Blueprint',
  'Quick Build',
  'Program entity',
  'Create new scene',
  'Create entity',
  'User interface',
  'Scene script',
  'Scene logic',
  'Tools',
  'Controls',
  'Resources',
  'Scenes',
  'viewport',
  'Construction',
  'Save',
]

const STEP_EMOJIS = ['👆', '⚙️', '🔧', '📋', '🎯']

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function normalizeUiText(value: string): string {
  return value.normalize('NFC')
}

function isInsideBoldMarker(text: string, offset: number): boolean {
  const before = text.slice(0, offset)
  return (before.match(/\*\*/g) ?? []).length % 2 === 1
}

function pruneSubstringLabels(labels: string[]): string[] {
  return labels.filter((label) => {
    const needle = label.toLowerCase()
    return !labels.some(
      (other) =>
        other !== label
        && other.length > label.length
        && other.toLowerCase().includes(needle),
    )
  })
}

function boldUiLabels(text: string, labels: string[]): string {
  const sorted = pruneSubstringLabels(labels.map(normalizeUiText)).sort(
    (a, b) => b.length - a.length,
  )
  const source = normalizeUiText(text)

  return source
    .split(/(\*\*[^*]+\*\*)/g)
    .map((segment) => {
      if (segment.startsWith('**') && segment.endsWith('**')) {
        return segment
      }

      let out = segment
      for (const label of sorted) {
        if (out.includes(`**${label}**`)) continue
        const re = new RegExp(escapeRegExp(label), 'gi')
        out = out.replace(re, (match, offset, whole) => {
          if (isInsideBoldMarker(whole, offset)) return match
          return `**${label}**`
        })
      }
      return out
    })
    .join('')
}

/** Separa pasos pegados en una sola línea: "1. a 2. b" → líneas distintas. */
function splitInlineNumberedSteps(text: string): string {
  return text
    .replace(/(?<=\S)\s+(?=\d+\.\s)/g, '\n\n')
    .replace(/(?<=\.)(?=\d+\.\s)/g, '\n\n')
}

function addStepEmojis(text: string): string {
  return text
    .split('\n')
    .map((line) => {
      const match = line.match(/^(\d+)\.\s*(.*)$/)
      if (!match) return line

      const stepNum = Number.parseInt(match[1], 10)
      const body = match[2].trim()
      const emoji = STEP_EMOJIS[stepNum - 1] ?? '▪️'

      if (STEP_EMOJIS.some((e) => body.startsWith(e))) {
        return line
      }

      return `${stepNum}. ${emoji} ${body}`
    })
    .join('\n')
}

function normalizeStepSpacing(text: string): string {
  const lines = text.split('\n')
  const blocks: string[] = []
  let buffer: string[] = []

  const flush = () => {
    if (buffer.length === 0) return
    blocks.push(buffer.join('\n'))
    buffer = []
  }

  for (const line of lines) {
    const trimmed = line.trim()
    if (/^\d+\.\s/.test(trimmed) && buffer.length > 0) {
      flush()
    }
    if (trimmed.length === 0) {
      flush()
      continue
    }
    buffer.push(trimmed)
  }
  flush()

  return blocks.join('\n\n')
}

const BLUEPRINT_CREATE_ES = `1. Clic en la entidad en el mundo.
2. Se abrirá la modal "Propiedades de la entidad" → Botón "Convertir en Blueprint" → Confirmar en modal.`

const BLUEPRINT_CREATE_EN = `1. Click the entity in the world.
2. "Entity properties" modal opens → "Convert to Blueprint" button → Confirm in modal.`

const BLUEPRINT_USE_ES = `1. Acordeón "Herramientas" → botón Construcción rápida.
2. Se abre la ventana modal → elegir categoría → elegir la blueprint.
3. Clic en el mundo para colocar copias.`

const BLUEPRINT_USE_EN = `1. Accordion "Tools" → Quick Build button.
2. Modal window opens → pick category → pick the blueprint.
3. Click in the world to place copies.`

const BLUEPRINT_CREATE_AND_USE_ES = `Crear una blueprint:
1. Clic en la entidad en el mundo.
2. Se abrirá la modal "Propiedades de la entidad" → Botón "Convertir en Blueprint" → Confirmar en modal.

Usar una blueprint:
1. Acordeón "Herramientas" → botón Construcción rápida.
2. Se abre la ventana modal → elegir categoría → elegir la blueprint.
3. Clic en el mundo para colocar copias.`

const BLUEPRINT_CREATE_AND_USE_EN = `Create a blueprint:
1. Click the entity in the world.
2. "Entity properties" modal opens → "Convert to Blueprint" button → Confirm in modal.

Use a blueprint:
1. Accordion "Tools" → Quick Build button.
2. Modal window opens → pick category → pick the blueprint.
3. Click in the world to place copies.`

function countNumberedSteps(text: string): number {
  return (text.match(/^\d+\.\s/gm) ?? []).length
}

function isBlueprintCreateAndUseQuestion(query: string): boolean {
  const q = query.toLowerCase()
  if (!/blueprint|plantilla/.test(q)) return false
  const wantsCreate = /crear|creo|convertir|create|convert/.test(q)
  const wantsUse = /\busar\b|\buso\b|colocar|place|construcci[oó]n|quick\s*build|r[aá]pida|copias?/.test(q)
  return wantsCreate && wantsUse
}

function isBlueprintCreateQuestion(query: string): boolean {
  if (isBlueprintCreateAndUseQuestion(query)) return false
  const q = query.toLowerCase()
  return /crear|creo|convertir|create|convert/.test(q) && /blueprint|plantilla/.test(q)
}

function isBlueprintUseQuestion(query: string): boolean {
  if (isBlueprintCreateAndUseQuestion(query)) return false
  const q = query.toLowerCase()
  return /blueprint|construcci[oó]n|quick\s*build|r[aá]pida|plantilla|copias?|usar|uso|use|colocar|place/.test(q)
}

/**
 * Aplica formato visual fiable tras la inferencia (negrita en UI, emojis, saltos de línea).
 * El modelo 1.7B no siempre respeta el formato; esto lo garantiza en el cliente.
 */
export function polishAssistantReply(
  text: string,
  locale: 'en' | 'es',
  userQuery = '',
): string {
  let result = text.trim()

  if (userQuery) {
    if (isBlueprintCreateAndUseQuestion(userQuery)) {
      result = locale === 'es' ? BLUEPRINT_CREATE_AND_USE_ES : BLUEPRINT_CREATE_AND_USE_EN
    } else if (countNumberedSteps(result) < 2) {
      if (isBlueprintCreateQuestion(userQuery)) {
        result = locale === 'es' ? BLUEPRINT_CREATE_ES : BLUEPRINT_CREATE_EN
      } else if (isBlueprintUseQuestion(userQuery)) {
        result = locale === 'es' ? BLUEPRINT_USE_ES : BLUEPRINT_USE_EN
      }
    }
  }

  if (!result) return result

  result = splitInlineNumberedSteps(result)
  result = normalizeStepSpacing(result)
  result = boldUiLabels(result, locale === 'es' ? UI_LABELS_ES : UI_LABELS_EN)
  result = addStepEmojis(result)
  result = result.replace(/\n{3,}/g, '\n\n').trim()

  return result
}
