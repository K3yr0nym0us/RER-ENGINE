const UI_LABELS_ES = [
  'Propiedades de la entidad',
  'Convertir en Blueprint',
  'Construcción Rápida',
  'Programar entidad',
  'Crear nueva escena',
  'Crear entidad',
  'Interfaz de usuario',
  'Scene script',
  'Scene logic',
  'Herramientas',
  'Controles',
  'Resources',
  'Escenas',
  'viewport',
  'Construcción',
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

function boldUiLabels(text: string, labels: string[]): string {
  let result = text
  const sorted = [...labels].sort((a, b) => b.length - a.length)
  for (const label of sorted) {
    if (result.includes(`**${label}**`)) continue
    const re = new RegExp(escapeRegExp(label), 'gi')
    result = result.replace(re, `**${label}**`)
  }
  return result
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

const BLUEPRINT_STEPS_ES = `1. Clic en la entidad en el viewport.
2. Propiedades de la entidad → Convertir en Blueprint.
3. Herramientas → botón Construcción Rápida.
4. Modal Construcción → elegir blueprint.
5. Clic en el viewport para colocar copias.`

const BLUEPRINT_STEPS_EN = `1. Click the entity in the viewport.
2. Entity properties → Convert to Blueprint.
3. Tools → Quick Build button.
4. Construction modal → pick a blueprint.
5. Click the viewport to place copies.`

function countNumberedSteps(text: string): number {
  return (text.match(/^\d+\.\s/gm) ?? []).length
}

function isBlueprintQuestion(query: string): boolean {
  const q = query.toLowerCase()
  return /blueprint|construcci[oó]n|quick\s*build|r[aá]pida|plantilla|copias?/.test(q)
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

  if (userQuery && isBlueprintQuestion(userQuery) && countNumberedSteps(result) < 4) {
    result = locale === 'es' ? BLUEPRINT_STEPS_ES : BLUEPRINT_STEPS_EN
  }

  if (!result) return result

  result = splitInlineNumberedSteps(result)
  result = normalizeStepSpacing(result)
  result = boldUiLabels(result, locale === 'es' ? UI_LABELS_ES : UI_LABELS_EN)
  result = addStepEmojis(result)
  result = result.replace(/\n{3,}/g, '\n\n').trim()

  return result
}
