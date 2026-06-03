import { useCallback, useEffect, useState } from 'react'

import type { ModalElectronOpenRequest } from '@shared-types'
import { LanguageProvider } from '../context/LanguageContext'
import { ModalElectronBody } from './ModalElectronBody'
import { ModalElectronCloseProvider } from './ModalElectronCloseContext'
import { useModalElectronContentSize } from './useModalElectronContentSize'

export function ModalElectronApp() {
  const [payload, setPayload] = useState<ModalElectronOpenRequest | null>(null)
  const contentRef = useModalElectronContentSize(payload != null)

  const closeModal = useCallback(() => {
    void window.electronAPI.closeModalElectron()
  }, [])

  useEffect(() => {
    const removeRender = window.electronAPI.onModalElectronRender((next) => {
      setPayload(next)
    })
    window.electronAPI.notifyModalElectronReady()
    return removeRender
  }, [])

  return (
    <LanguageProvider>
      <ModalElectronCloseProvider closeModal={closeModal}>
        <div
          ref={contentRef}
          className="p-3"
          style={{ background: 'var(--bs-body-bg)', boxSizing: 'border-box' }}
        >
          {payload ? (
            <ModalElectronBody
              key={payload.handlerId}
              payload={payload}
              onClose={closeModal}
            />
          ) : (
            <p className="text-secondary small mb-0">…</p>
          )}
        </div>
      </ModalElectronCloseProvider>
    </LanguageProvider>
  )
}
