import { useCallback } from 'react';
import { Box } from 'react-bootstrap-icons';
import ModalSetNameModel from './ModalSetNameModel';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import type { ModelCategory } from '@shared-types';

interface BtnLoadModelProps {
  category: ModelCategory;
}

export const BtnLoadModel = ({ category }: BtnLoadModelProps) => {
  const { t } = useTraslate();
  const { openModal } = useModal();
  const { loadModelAsset } = useContextEngine();

  const openLoadModelModal = useCallback(async () => {
    const path = await window.electronAPI.openModelDialog();
    if (!path) return;
    const base = path.split(/[/\\]/).pop() ?? 'model';
    const autoName = base.replace(/\.[^/.]+$/, '');
    openModal({
      title: t('Assign name to model'),
      body: (
        <ModalSetNameModel
          path={path}
          autoName={autoName}
          category={category}
          onConfirm={({ path: modelPath, name, category: modelCategory }) => {
            loadModelAsset(modelPath, name, modelCategory);
          }}
        />
      ),
    });
  }, [category, loadModelAsset, openModal, t]);

  return (
    <button
      className="btn btn-outline-primary btn-sm w-100 mb-2"
      type="button"
      onClick={openLoadModelModal}
    >
      <Box className="me-1" /> {t('Load model (.glb/.gltf/.fbx)')}
    </button>
  );
};

export default BtnLoadModel;
