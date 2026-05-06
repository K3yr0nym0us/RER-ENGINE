export function UserGuide() {
  return (
    <div className="d-flex flex-column gap-3">
      <section>
        <h5 className="mb-1">Inicio rapido</h5>
        <p className="text-secondary mb-0">
          Empieza creando recursos o entidades desde los accordiones de la barra lateral izquierda.
        </p>
      </section>

      <section>
        <h5 className="mb-1">Flujo recomendado</h5>
        <ol className="mb-0 text-secondary">
          <li>Define primero el mundo, su tamaño y el de la cuadricula.</li>
          <li>Carga al motor Sprites o SpriteSheets en el accordion de sprites.</li>
          <li>Crea entidades y ajusta sus propiedades en el accordion de entidades.</li>
          <li>Haz click en una entidad para editar el ente en el accordion de Propiedades.</li>
          <li>Crea blueprints basadas en entidades para entidades repetitivas.</li>
          <li>Usa la herramienta de construcción rápida del accordion de herramientas para usar las blueprints.</li>
          <li>Guarda frecuentemente para no perder avances.</li>
        </ol>
      </section>

      <section>
        <h5 className="mb-1">Navegacion del editor</h5>
        <p className="text-secondary mb-1">
          La barra lateral izquierda concentra la configuracion principal:
        </p>
        <ul className="mb-0 text-secondary">
          <li><strong>Mundo:</strong> tamano de area de trabajo, fondo, cuadricula y gravedad.</li>
          <li><strong>Sprites:</strong> carga y organizacion de recursos visuales 2D.</li>
          <li><strong>Entidades:</strong> creacion y gestion de entidades (Personajes, Elementos del escenario, objetos, etc.)</li>
          <li><strong>Tools:</strong> herramientas rapidas de construccion y trabajo (Muros invisibles, triggers de codigo, etc).</li>
          <li><strong>Controls:</strong> configuracion de los controles del juego y personaje.</li>
        </ul>
      </section>

      <section>
        <h5 className="mb-1">Guardado y proyecto</h5>
        <p className="text-secondary mb-0">
          Usa el boton superior derecho para guardar el proyecto. Una vez guardado se activará el guardado automatico.
        </p>
      </section>
    </div>
  )
}

export default UserGuide