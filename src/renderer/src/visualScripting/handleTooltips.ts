import {
  EXEC_IN,
  EXEC_OUT,
  LOOP_BODY,
  THEN_0,
  THEN_1,
  THEN_FALSE,
  THEN_TRUE,
} from './nodeDefinitions'

/** Claves i18n (inglés) para tooltip de cada pin de ejecución. */
export const HANDLE_TOOLTIP_KEYS: Record<string, string> = {
  [EXEC_IN]: 'Handle exec in',
  [EXEC_OUT]: 'Handle exec out',
  [THEN_0]: 'Handle then 0',
  [THEN_1]: 'Handle then 1',
  [THEN_TRUE]: 'Handle then true',
  [THEN_FALSE]: 'Handle then false',
  [LOOP_BODY]: 'Handle loop body',
}
