import { Accordion } from 'react-bootstrap';
import { Image, PencilSquare, Trash } from 'react-bootstrap-icons';
import { BtnLoadSprite } from './components/BtnLoadSprite';

export interface Sprite {
  id: string;
  name: string;
  src: string;
}

interface SpritesAccordionProps {
  sprites: Sprite[];
  onAddSprite: (sprite: Omit<Sprite, 'id'>) => void;
}

const SpritesAccordion = ({ sprites, onAddSprite }: SpritesAccordionProps) => (
  <Accordion.Item eventKey="sprites">
    <Accordion.Header>Sprites</Accordion.Header>
    <Accordion.Body className="py-2 px-2">
      <BtnLoadSprite onSpriteLoaded={onAddSprite} />
      <ul className="list-unstyled mt-2 mb-0">
        {sprites.length === 0 && <li className="text-muted">Sin sprites cargados</li>}
        {sprites.map(sprite => (
          <li key={sprite.id} className="mb-1">
            <span className="d-flex align-items-center justify-content-between gap-2 border rounded p-2">
              <div>
                <Image className="me-2" />
                {sprite.name}
              </div>
              <div>
                <button 
                  className="btn btn-sm text-warning"
                  title="Editar Sprite"
                >
                  <PencilSquare />
                </button>
                <button 
                  className="btn btn-sm text-danger"
                  title="Eliminar Sprite"
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

export default SpritesAccordion;
