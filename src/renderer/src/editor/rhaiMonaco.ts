import './monacoSetup'
import type { languages } from 'monaco-editor'

export const RHAI_MONACO_LANGUAGE = 'rhai'

const rhaiTokenizer: languages.IMonarchLanguage = {
  defaultToken: '',
  tokenPostfix: '.rhai',
  keywords: [
    'fn', 'let', 'const', 'if', 'else', 'switch', 'case', 'default',
    'loop', 'while', 'do', 'for', 'in', 'break', 'continue', 'return',
    'throw', 'try', 'catch', 'true', 'false', 'pub', 'private', 'use',
    'import', 'as', 'go_to', 'print', 'debug', 'eval', 'is', 'match',
  ],
  typeKeywords: [],
  operators: [
    '=', '>', '<', '!', '~', '?', ':', '==', '<=', '>=', '!=',
    '&&', '||', '++', '--', '+', '-', '*', '/', '&', '|', '^', '%',
    '<<', '>>', '>>>', '+=', '-=', '*=', '/=', '&=', '|=', '^=',
    '%=', '<<=', '>>=', '>>>=', '=>', '..',
  ],
  symbols: /[=><!~?:&|+\-*/^%]+/,
  escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,
  tokenizer: {
    root: [
      [/\/\/.*$/, 'comment'],
      [/\/\*/, 'comment', '@comment'],
      [/[{}()[\]]/, '@brackets'],
      [
        /[a-zA-Z_]\w*/,
        {
          cases: {
            '@keywords': 'keyword',
            '@default': 'identifier',
          },
        },
      ],
      [/"([^"\\]|\\.)*$/, 'string.invalid'],
      [/"/, 'string', '@string'],
      [/'[^\\']'/, 'string'],
      [/\d*\.\d+([eE][-+]?\d+)?/, 'number.float'],
      [/\d+/, 'number'],
      [/[;,.]/, 'delimiter'],
      [
        /@symbols/,
        {
          cases: {
            '@operators': 'operator',
            '@default': '',
          },
        },
      ],
      [/\s+/, 'white'],
    ],
    comment: [
      [/[^/*]+/, 'comment'],
      [/\*\//, 'comment', '@pop'],
      [/[/*]/, 'comment'],
    ],
    string: [
      [/[^\\"]+/, 'string'],
      [/@escapes/, 'string.escape'],
      [/\\./, 'string.escape.invalid'],
      [/"/, 'string', '@pop'],
    ],
  },
}

let registered = false

/** Registra el lenguaje Rhai en Monaco (idempotente). Usar en `beforeMount`. */
export function registerRhaiMonacoLanguage(monaco: typeof import('monaco-editor')): void {
  if (registered) return
  registered = true

  monaco.languages.register({ id: RHAI_MONACO_LANGUAGE })

  monaco.languages.setLanguageConfiguration(RHAI_MONACO_LANGUAGE, {
    comments: {
      lineComment: '//',
      blockComment: ['/*', '*/'],
    },
    brackets: [
      ['{', '}'],
      ['[', ']'],
      ['(', ')'],
    ],
    autoClosingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
    surroundingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
    folding: {
      markers: {
        start: /^\s*\/\//,
        end: /^\s*\/\//,
      },
    },
  })

  monaco.languages.setMonarchTokensProvider(RHAI_MONACO_LANGUAGE, rhaiTokenizer)
}

export const RHAI_MONACO_EDITOR_OPTIONS = {
  fontSize: 13,
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  wordWrap: 'on' as const,
  tabSize: 2,
  insertSpaces: true,
  automaticLayout: true,
  lineNumbersMinChars: 3,
  padding: { top: 8 },
}
