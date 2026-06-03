import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';
import type { HudImageInfo } from '@shared-types';
import { useModalClose } from '../../../../../../modal-electron/useModalClose';

interface ModalSelectHudImageProps {
  onSelect: (imagePath: string) => void;
  hudImages?: HudImageInfo[];
  onClose?: () => void;
}

function ModalSelectHudImageInner({
  onSelect,
  hudImages,
  onClose,
}: ModalSelectHudImageProps & { hudImages: HudImageInfo[]; onClose: () => void }) {
  const { t } = useTraslate();

  if (hudImages.length === 0) {
    return (
      <p className="text-muted small mb-0">
        {t('No images loaded. Add images in Resources first.')}
      </p>
    );
  }

  return (
    <ul className="list-unstyled mb-0">
      {hudImages.map((img) => (
        <li key={img.path} className="mb-2">
          <button
            className="btn btn-outline-light btn-sm w-100 text-start"
            type="button"
            onClick={() => {
              onSelect(img.path);
              onClose();
            }}
          >
            {img.name}
          </button>
        </li>
      ))}
    </ul>
  );
}

function ModalSelectHudImageWithEngine(props: ModalSelectHudImageProps) {
  const { hudImages } = useContextEngine();
  const closeModal = useModalClose();
  return (
    <ModalSelectHudImageInner
      {...props}
      hudImages={props.hudImages ?? hudImages}
      onClose={props.onClose ?? closeModal}
    />
  );
}

export default function ModalSelectHudImage(props: ModalSelectHudImageProps) {
  if (props.hudImages) {
    const onClose = props.onClose ?? (() => {});
    return <ModalSelectHudImageInner {...props} hudImages={props.hudImages} onClose={onClose} />;
  }
  return <ModalSelectHudImageWithEngine {...props} />;
}
