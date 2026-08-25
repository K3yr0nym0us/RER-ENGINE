import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ComponentProps,
  type ReactNode,
} from 'react'
import type { Accordion } from 'react-bootstrap'

import type { PluginUiAction } from '@shared-types'
import { usePluginUiActionBridge } from '../plugins/usePluginUiActionBridge'

type AccordionSelectKey = Parameters<
  NonNullable<ComponentProps<typeof Accordion>['onSelect']>
>[0]

interface SidebarAccordionContextValue {
  propsFor: (key: string) => {
    activeKey: string | undefined
    onSelect: (next: AccordionSelectKey) => void
    className: 'sidebar-accordion'
  }
  openAccordion: (key: string) => void
  highlightTarget: (targetId: string) => void
}

const SidebarAccordionContext = createContext<SidebarAccordionContextValue | null>(null)

export function SidebarAccordionProvider({ children }: { children: ReactNode }) {
  usePluginUiActionBridge()
  const [activeKey, setActiveKey] = useState<string | null>('scenes')

  const openAccordion = useCallback((key: string) => {
    setActiveKey(key)
  }, [])

  const highlightTarget = useCallback((targetId: string) => {
    const el = document.querySelector(`[data-plugin-target="${targetId}"]`)
    if (!el || !(el instanceof HTMLElement)) return
    el.classList.add('plugin-ui-highlight')
    el.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    window.setTimeout(() => el.classList.remove('plugin-ui-highlight'), 3_000)
  }, [])

  useEffect(() => {
    const handler = (event: Event) => {
      const action = (event as CustomEvent<PluginUiAction>).detail
      if (!action) return
      if (action.type === 'open_sidebar_accordion') {
        openAccordion(action.accordionKey)
      }
      if (action.type === 'highlight_ui_target') {
        highlightTarget(action.targetId)
      }
    }
    window.addEventListener('plugins:ui-action', handler)
    return () => window.removeEventListener('plugins:ui-action', handler)
  }, [openAccordion, highlightTarget])

  const propsFor = useCallback(
    (key: string) => ({
      activeKey: activeKey === key ? key : undefined,
      onSelect: (next: AccordionSelectKey) => {
        setActiveKey(typeof next === 'string' ? next : null)
      },
      className: 'sidebar-accordion' as const,
    }),
    [activeKey],
  )

  const value = useMemo(
    () => ({ propsFor, openAccordion, highlightTarget }),
    [propsFor, openAccordion, highlightTarget],
  )

  return (
    <SidebarAccordionContext.Provider value={value}>{children}</SidebarAccordionContext.Provider>
  )
}

export function useSidebarAccordion(): SidebarAccordionContextValue {
  const ctx = useContext(SidebarAccordionContext)
  if (!ctx) {
    throw new Error('useSidebarAccordion must be used within SidebarAccordionProvider')
  }
  return ctx
}
