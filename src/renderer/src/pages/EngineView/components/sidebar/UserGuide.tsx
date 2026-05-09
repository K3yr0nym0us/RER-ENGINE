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
          <li>Define primero el mundo, su tamaño y el de la cuadricula en el accordion de "Mundo".</li>
          <li>Carga al motor recursos como Sprites, sonidos, fondos, en el accordion de "Recursos".</li>
          <li>Crea entidades y ajusta sus propiedades en el accordion de "Entidades".</li>
          <li>Haz click en una entidad para editarla en el accordion de "Propiedades".</li>
          <li>Crea blueprints basadas en entidades para entidades repetitivas y asi agilizar el desarrollo.</li>
          <li>Usa la herramienta de construcción rápida del accordion de "Herramientas" para usar las blueprints.</li>
          <li>Guarda frecuentemente para no perder avances (O activa el guardado automático).</li>
        </ol>
      </section>

      <section>
        <h5 className="mb-1">Navegacion del editor</h5>
        <p className="text-secondary mb-1">
          La barra lateral izquierda concentra la configuracion principal:
        </p>
        <ul className="mb-0 text-secondary">
          <li><strong>Mundo:</strong> tamano de area de trabajo, fondo, cuadricula y gravedad.</li>
          <li><strong>Recursos:</strong> carga y organizacion de recursos visuales 2D (Sprites, Sounds, Backgrounds).</li>
          <li><strong>Entidades:</strong> creacion y gestion de entidades (Personajes, Elementos del escenario, objetos, etc.)</li>
          <li><strong>Herramientas:</strong> herramientas rapidas de construccion y trabajo (Muros invisibles, triggers de codigo, etc).</li>
          <li><strong>Controles:</strong> configuracion de los controles del juego y personaje.</li>
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