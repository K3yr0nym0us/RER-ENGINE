import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';

interface ModalSelectHudImageProps {
  onSelect: (imagePath: string) => void;
}

export default function ModalSelectHudImage({ onSelect }: ModalSelectHudImageProps) {
  const { t } = useTraslate();
  const { hudImages } = useContextEngine();
  const { closeModal } = useModal();

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
              closeModal();
            }}
          >
            {img.name}
          </button>
        </li>
      ))}
    </ul>
  );
}
