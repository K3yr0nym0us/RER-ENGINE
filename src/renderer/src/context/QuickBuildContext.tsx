import { createContext, useContext, useState, type ReactNode } from 'react'
import type { BluePrintEntry } from '@shared-types'

interface QuickBuildContextValue {
  activeBluePrint: BluePrintEntry | null
  setActiveBluePrint: (bp: BluePrintEntry | null) => void
}

const QuickBuildContext = createContext<QuickBuildContextValue | null>(null)

export function QuickBuildProvider({ children }: { children: ReactNode }) {
  const [activeBluePrint, setActiveBluePrint] = useState<BluePrintEntry | null>(null)

  return (
    <QuickBuildContext.Provider value={{ activeBluePrint, setActiveBluePrint }}>
      {children}
    </QuickBuildContext.Provider>
  )
}

export function useQuickBuild(): QuickBuildContextValue {
  const ctx = useContext(QuickBuildContext)
  if (!ctx) throw new Error('useQuickBuild debe usarse dentro de <QuickBuildProvider>')
  return ctx
}
