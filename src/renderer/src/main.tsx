import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { LanguageProvider } from './context/LanguageContext';
import { ModalElectronApp } from './modal-electron/ModalElectronApp';
import { AiAssistantOverlayApp } from './ai-overlay/AiAssistantOverlayApp';

import 'bootstrap/dist/css/bootstrap.min.css';
import './styles/index.css';

const isModalElectronHost =
  window.location.hash === '#/modal-electron' ||
  window.location.search.includes('modalElectron=1');

const isAiAssistantOverlayHost = window.location.hash === '#/ai-assistant-overlay';

const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);

if (isAiAssistantOverlayHost) {
  root.render(<AiAssistantOverlayApp />);
} else if (isModalElectronHost) {
  root.render(<ModalElectronApp />);
} else {
  root.render(
    /* <React.StrictMode> */
      <LanguageProvider>
        <App />
      </LanguageProvider>
    /* </React.StrictMode>, */
  );
}

