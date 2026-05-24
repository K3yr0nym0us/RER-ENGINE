export function UserGuide() {
  return (
    <div className="d-flex flex-column gap-3">
      <section>
        <h5 className="mb-1">Flujo recomendado</h5>
        <ol className="mb-0 text-secondary">
          <li>Crea o selecciona una escena en el acordeón de &quot;Escenas&quot;.</li>
          <li>Define el mundo: tamaño, cuadrícula, etc en el accordion de &quot;Mundo&quot;.</li>
          <li>Carga recursos al motor (sprites o modelos, sonidos y fondos en 2D) desde &quot;Recursos&quot;.</li>
          <li>Crea entidades como: entorno, personajes y objetos en el accordion de &quot;Entidades&quot;.</li>
          <li>Haz clic en una entidad para editarla en el accordion de &quot;Propiedades&quot;.</li>
          <li>Convierte entidades a blueprints para reutilizarlas en el accordion de &quot;Propiedades&quot; boton &quot;Convertir en Blueprint&quot;.</li>
          <li>Usa la herramienta de construcción rápida para usar las blueprints en el accordion de &quot;Herramientas&quot;.</li>
          <li>Guarda con frecuencia o activa el guardado automático.</li>
        </ol>
      </section>

      <section>
        <h5 className="mb-1">Navegacion del editor</h5>
        <p className="text-secondary mb-1">
          La barra lateral izquierda concentra la configuracion principal:
        </p>
        <ul className="mb-0 text-secondary">
          <li><strong>Escenas:</strong> lista de escenas del proyecto; crear, renombrar, eliminar y cambiar la escena activa.</li>
          <li><strong>Mundo:</strong> tamaño del area de trabajo, cuadrícula, gravedad, FPS objetivo y (2D) fondo del nivel; en 3D también luz direccional y sombras.</li>
          <li><strong>Cámara:</strong> en 2D, posición y zoom de la cámara del editor; en 3D FP, ojo de cámara, FOV, frustum y modo de seguimiento al personaje.</li>
          <li><strong>Recursos:</strong> carga y organización de assets — en 2D: sprites, sonidos y fondos; en 3D: modelos 3D y sonidos.</li>
          <li><strong>Entidades:</strong> creación de entorno, personajes y objetos a partir de los recursos cargados.</li>
          <li><strong>Herramientas (2D):</strong> dibujar colisionadores, áreas de ejecución de scripts y colocar blueprints con construcción rápida.</li>
          <li><strong>Controles:</strong> elige un personaje y configura teclas, mouse o mandos con scripts Lua.</li>
          <li><strong>Propiedades:</strong> aparece al seleccionar una entidad; nombre, transform, física, animaciones, scripts y acciones (eliminar, blueprint, etc.).</li>
        </ul>
      </section>

      <section>
        <h5 className="mb-1">Guardado y proyecto</h5>
        <p className="text-secondary mb-0">
          Usa el boton superior derecho para guardar el proyecto. Una vez guardado se activará el guardado automatico.
          Cada escena conserva sus propias entidades y configuración de mundo.
        </p>
      </section>
    </div>
  )
}

export default UserGuide
