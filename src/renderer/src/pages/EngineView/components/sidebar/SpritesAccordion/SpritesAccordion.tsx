import { Accordion } from 'react-bootstrap';
import { Image, Trash } from 'react-bootstrap-icons';

import { useContextEngine } from '../../../../../context/useContextEngine';
import { BtnLoadSprite } from './components/BtnLoadSprite';
import { useModal } from '../../../../../context/ModalContext';

type Sprite = {
  height: number;
  name: string;
  path: string;
  width: number;
}

const SpritesAccordion = () => {
  const { sprites, removeSprite } = useContextEngine();
  const { openModal, closeModal } = useModal();

  const handleDeleteSprite = (sprite: Sprite) => {
    openModal({
      title: 'Eliminar Sprite',
      body: (
        <div className="text-center">
          <p>¿Estás seguro de que deseas eliminar el sprite <strong>{sprite.name}</strong>?</p>
          <p>Se eliminara toda configuracion vinculada con el Sprite.</p>
          <p className="text-danger">Esta acción no se puede deshacer.</p>
          <div className="d-flex justify-content-end gap-2 mt-3">
            <button
              className="btn btn-secondary"
              onClick={() => closeModal()}
            >
              Cancelar
            </button>
            <button
              className="btn btn-danger"
              onClick={() => {
                removeSprite(sprite.path);
                closeModal();
              }}
            >
              Si, Eliminar
            </button>
          </div>
        </div>
      )
    })
  }

  return (
    <Accordion.Item eventKey="sprites">
      <Accordion.Header>Sprites</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <BtnLoadSprite />
        <ul className="list-unstyled mt-2 mb-0">
          {sprites.length === 0 && <li className="text-muted">Sin sprites cargados</li>}
          {sprites.map((sprite) => (
            <li key={sprite.path} className="mb-1">
              <span className="d-flex align-items-center justify-content-between gap-2 border rounded p-2">
                <div>
                  <Image className="me-2" />
                  {sprite.name}
                </div>
                <div>
                  <button 
                    className="btn btn-sm text-danger"
                    title="Eliminar Sprite"
                    onClick={() => handleDeleteSprite(sprite)}
                  >
                    <Trash />
                  </button>
                </div>
              </span>
            </li>
          ))}
        </ul>
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default SpritesAccordion;
