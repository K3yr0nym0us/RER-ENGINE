import type { ModelInfo } from '@shared-types';
import { Spinner } from 'react-bootstrap';
import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';

interface Props {
  model: ModelInfo;
  onSelect: (path: string) => void;
}

function EntityLoadingTooltip() {
  const { t } = useTraslate();

  return (
    <span className="d-block text-start" style={{ maxWidth: 220 }}>
      <span className="d-block">{t('Entity is still loading,')}</span>
      <span className="d-block">{t('please wait a moment before using it.')}</span>
    </span>
  );
}

/** Fila de selector de modelo con spinner de precarga reactivo al contexto del motor. */
export function ModelPickerEntry({ model, onSelect }: Props) {
  const { t } = useTraslate();
  const isLoading = model.loading === true;
  const loadingLabel = `${t('Entity is still loading,')} ${t('please wait a moment before using it.')}`;

  const button = (
    <button
      type="button"
      className="btn btn-outline-light btn-sm w-100 text-start d-flex align-items-center justify-content-between gap-2"
      disabled={isLoading}
      style={isLoading ? { pointerEvents: 'none' } : undefined}
      onClick={() => onSelect(model.path)}
    >
      <span className="text-truncate">{model.name}</span>
      {isLoading ? (
        <Spinner
          animation="border"
          size="sm"
          variant="primary"
          role="status"
          aria-label={loadingLabel}
        />
      ) : null}
    </button>
  );

  return (
    <li className="mb-2">
      {isLoading ? (
        <AppTooltip content={<EntityLoadingTooltip />} place="left">
          <span className="d-block w-100" tabIndex={0} style={{ cursor: 'not-allowed' }}>
            {button}
          </span>
        </AppTooltip>
      ) : (
        button
      )}
    </li>
  );
}

export default ModelPickerEntry;
