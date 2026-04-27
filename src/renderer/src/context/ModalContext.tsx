import { createContext, useCallback, useContext, useState, type ReactNode } from 'react'
import { Modal } from 'react-bootstrap'

// ---------------------------------------------------------------------------
// Tipos
// ---------------------------------------------------------------------------

export type ModalSize = 'sm' | 'lg' | 'xl'

export interface ModalConfig {
  title: string
  body:  ReactNode
  size?: ModalSize
}

interface ModalContextType {
  openModal:  (config: ModalConfig) => void
  closeModal: () => void
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

const ModalContext = createContext<ModalContextType | null>(null)

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function ModalProvider({ children }: { children: ReactNode }) {
  const [config, setConfig] = useState<ModalConfig | null>(null)

  const openModal = useCallback((cfg: ModalConfig) => {
    ;(window as any).electronAPI?.hideEngineViewport?.()
    setConfig(cfg)
  }, [])

  const closeModal = useCallback(() => {
    setConfig(null)
    ;(window as any).electronAPI?.restoreEngineViewport?.()
  }, [])

  return (
    <ModalContext.Provider value={{ openModal, closeModal }}>
      {children}

      <Modal
        show={config !== null}
        onHide={closeModal}
        size={config?.size}
        centered
      >
        {config && (
          <>
            <Modal.Header closeButton>
              <Modal.Title>{config.title}</Modal.Title>
            </Modal.Header>
            <Modal.Body>{config.body}</Modal.Body>
          </>
        )}
      </Modal>
    </ModalContext.Provider>
  )
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useModal() {
  const ctx = useContext(ModalContext)
  if (!ctx) throw new Error('useModal debe usarse dentro de ModalProvider')
  return ctx
}
