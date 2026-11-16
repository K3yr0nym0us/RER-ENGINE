import { Component, type ErrorInfo, type ReactNode } from 'react';

interface ModalElectronErrorBoundaryProps {
  children: ReactNode;
}

interface ModalElectronErrorBoundaryState {
  error: Error | null;
}

/**
 * Error boundary de la ventana modal Electron.
 *
 * La ventana modal es una instancia persistente reutilizada: si un cuerpo
 * lanza en render sin boundary, React desmonta el árbol completo y TODAS las
 * modales siguientes se ven vacías. Con el boundary, el fallo queda aislado
 * en la modal que lo provocó y la ventana sigue utilizable.
 */
class ModalElectronErrorBoundary extends Component<
  ModalElectronErrorBoundaryProps,
  ModalElectronErrorBoundaryState
> {
  state: ModalElectronErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ModalElectronErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('[modal-electron] error renderizando el cuerpo del modal:', error, info.componentStack);
  }

  render(): ReactNode {
    if (this.state.error) {
      return (
        <p className="text-danger small mb-0">
          Error en la modal: <code>{this.state.error.message}</code>
        </p>
      );
    }
    return this.props.children;
  }
}

export default ModalElectronErrorBoundary;
