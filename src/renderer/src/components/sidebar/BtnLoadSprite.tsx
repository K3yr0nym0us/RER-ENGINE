import { Image } from 'react-bootstrap-icons';
import { useModal } from '../../context/ModalContext';
import { SpritePreviewModalBody } from './SpritePreviewModalBody';

export function BtnLoadSprite() {
  const { openModal } = useModal();

  const handleLoadSprite = async () => {
    // Abre el diálogo de archivos para seleccionar una imagen
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = 'image/*'
    input.onchange = () => {
      const file = input.files?.[0]
      if (file) {
        const reader = new FileReader()
        reader.onload = () => {
          // Abre el modal mostrando la imagen seleccionada
          openModal({
            title: 'Vista previa del Sprite',
            body: <SpritePreviewModalBody src={reader.result as string} />,
            size: 'xl',
          })
        }
        reader.readAsDataURL(file)
      }
    }
    input.click()
  }

  return (
    <button
      className="btn btn-outline-primary btn-sm w-100 mb-2"
      type="button"
      onClick={handleLoadSprite}
    >
      <Image className="me-1" /> Cargar Sprite
    </button>
  )
}

export default BtnLoadSprite