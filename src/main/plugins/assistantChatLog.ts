import fs from 'fs'
import path from 'path'
import { app } from 'electron'

const LOG_PREFIX = '[ai-assistant]'
const MAX_LOG_CHARS = 1_200

function logFilePath(): string {
  return path.join(app.getPath('userData'), 'plugins', 'ai-assistant-chat.log')
}

function truncate(value: string, max = MAX_LOG_CHARS): string {
  if (value.length <= max) return value
  return `${value.slice(0, max)}… (${value.length} chars total)`
}

function serialize(data: unknown): string {
  try {
    return JSON.stringify(
      data,
      (_key, value) => {
        if (typeof value === 'string') return truncate(value)
        return value
      },
      2,
    )
  } catch {
    return String(data)
  }
}

function appendToFile(line: string): void {
  try {
    const file = logFilePath()
    fs.mkdirSync(path.dirname(file), { recursive: true })
    const stamp = new Date().toISOString()
    fs.appendFileSync(file, `[${stamp}] ${line}\n`, 'utf8')
  } catch {
    // ignore file log errors
  }
}

export function aiLog(section: string, data?: unknown): void {
  const message = data === undefined ? section : `${section}\n${serialize(data)}`
  console.log(LOG_PREFIX, message)
  appendToFile(message)
}

export function getAiAssistantLogFilePath(): string {
  return logFilePath()
}
