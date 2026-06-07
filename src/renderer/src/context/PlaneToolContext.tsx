import { createContext, useContext, useState, type ReactNode } from 'react'

export type PlaneToolName = 'draw_collider' | 'draw_execution_area'

interface PlaneToolContextValue {
  activePlaneTool: PlaneToolName | null
  setActivePlaneTool: (
    value: PlaneToolName | null | ((prev: PlaneToolName | null) => PlaneToolName | null),
  ) => void
}

const PlaneToolContext = createContext<PlaneToolContextValue | null>(null)

export const DEFAULT_PLANE_WIDTH = 4
export const DEFAULT_PLANE_HEIGHT = 3
/** Profundidad de colisión enviada al motor (el mesh visual es un quad delgado). */
export const PLANE_TOOL_DEPTH = 0.05

export function PlaneToolProvider({ children }: { children: ReactNode }) {
  const [activePlaneTool, setActivePlaneTool] = useState<PlaneToolName | null>(null)

  return (
    <PlaneToolContext.Provider value={{ activePlaneTool, setActivePlaneTool }}>
      {children}
    </PlaneToolContext.Provider>
  )
}

export function usePlaneTool(): PlaneToolContextValue {
  const ctx = useContext(PlaneToolContext)
  if (!ctx) throw new Error('usePlaneTool debe usarse dentro de <PlaneToolProvider>')
  return ctx
}
