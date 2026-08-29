import { useCallback, useEffect, useState } from 'react'

import type { ModalElectronOpenRequest } from '@shared-types'
import { LanguageProvider } from '../context/LanguageContext'
import { isBlockingOverlayModalComponent } from './modalElectronLayout'
import { ModalElectronBody } from './ModalElectronBody'
import { ModalElectronCloseProvider } from './ModalElectronCloseContext'
import { useModalElectronContentSize } from './useModalElectronContentSize'

export function ModalElectronApp() {
  const [payload, setPayload] = useState<ModalElectronOpenRequest | null>(null)
  const contentRef = useModalElectronContentSize(
    payload != null,
    payload?.componentKey,
    payload?.resizable,
    payload?.blockingOverlay,
  )

  const isResizable = payload?.resizable === true
  const isBlockingOverlay = isBlockingOverlayModalComponent(
    payload?.componentKey,
    payload?.blockingOverlay,
  )

  useEffect(() => {
    document.documentElement.classList.toggle('modal-electron-resizable', isResizable)
    document.body.classList.toggle('modal-electron-resizable', isResizable)
    return () => {
      document.documentElement.classList.remove('modal-electron-resizable')
      document.body.classList.remove('modal-electron-resizable')
    }
  }, [isResizable])

  const closeModal = useCallback(() => {
    void window.electronAPI.closeModalElectron()
  }, [])

  useEffect(() => {
    const removeRender = window.electronAPI.onModalElectronRender((next) => {
      setPayload(next ?? null)
    })
    window.electronAPI.notifyModalElectronReady()
    return removeRender
  }, [])

  const modalLocale = payload?.locale === 'es' ? 'es' : 'en'

  return (
    <LanguageProvider key={payload?.handlerId ?? 'idle'} initialLocale={modalLocale}>
      <ModalElectronCloseProvider closeModal={closeModal}>
        <div
          ref={contentRef}
          className={
            isBlockingOverlay
              ? 'modal-electron-shell--blocking'
              : `p-3${isResizable ? ' modal-electron-shell--resizable' : ''}`
          }
          style={{
            background: isBlockingOverlay ? 'transparent' : 'var(--bs-body-bg)',
            boxSizing: 'border-box',
            ...(isResizable
              ? { height: '100vh', display: 'flex', flexDirection: 'column', overflow: 'hidden' }
              : isBlockingOverlay
                ? { height: '100vh', overflow: 'hidden' }
                : {}),
          }}
        >
          {payload ? (
            <ModalElectronBody
              key={payload.handlerId}
              payload={payload}
              onClose={closeModal}
            />
          ) : (
            <p className="text-secondary small mb-0">{modalLocale === 'es' ? 'Cargando…' : 'Loading…'}</p>
          )}
        </div>
      </ModalElectronCloseProvider>
    </LanguageProvider>
  )
}
