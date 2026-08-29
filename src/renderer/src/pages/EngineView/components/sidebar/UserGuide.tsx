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
          <li>Programa la escena desde <strong>Escenas → Programación</strong> (nodos visuales o script Rhai de escena).</li>
          <li>Programa una entidad desde <strong>Propiedades → Programar entidad</strong> (nodos o scripts Rhai por entidad).</li>
          <li>Guarda con frecuencia o activa el guardado automático.</li>
        </ol>
      </section>

      <section>
        <h5 className="mb-1">Navegacion del editor</h5>
        <p className="text-secondary mb-1">
          La barra lateral izquierda concentra la configuracion principal:
        </p>
        <ul className="mb-0 text-secondary">
          <li><strong>Escenas:</strong> lista de escenas del proyecto; crear, renombrar, eliminar y cambiar la escena activa. Dentro, <strong>Programación</strong> abre la lógica de escena (nodos o Rhai).</li>
          <li><strong>Mundo:</strong> tamaño del area de trabajo, cuadrícula, gravedad, FPS objetivo y (2D) fondo del nivel; en 3D también luz direccional y sombras.</li>
          <li><strong>Cámara:</strong> en 2D, posición y zoom de la cámara del editor; en 3D, selector de tipo de cámara y controles del ojo (posición, FOV, frustum) cuando la cámara es play character (primera persona).</li>
          <li><strong>Recursos:</strong> carga y organización de assets — en 2D: sprites, sonidos y fondos; en 3D: modelos 3D y sonidos.</li>
          <li><strong>Entidades:</strong> creación de entorno, personajes y objetos a partir de los recursos cargados.</li>
          <li><strong>Herramientas:</strong> en 2D, dibujar colisionadores, áreas de ejecución y construcción rápida con blueprints; en 3D, construcción rápida con blueprints.</li>
          <li><strong>Controles:</strong> elige un personaje y configura teclas, mouse o mandos con scripts Rhai.</li>
          <li><strong>Propiedades:</strong> aparece al seleccionar una entidad; nombre, transform, física, animaciones, <strong>Programar entidad</strong> (nodos o scripts Rhai) y acciones (eliminar, blueprint, etc.).</li>
        </ul>
      </section>

      <section>
        <h5 className="mb-1">Viewport 3D (estilo Blender)</h5>
        <p className="text-secondary mb-1">
          En proyectos 3D, la navegación y el movimiento de entidades siguen el esquema por defecto de Blender:
        </p>
        <ul className="mb-0 text-secondary">
          <li><strong>Órbita:</strong> botón central del ratón (MMB) + arrastre.</li>
          <li><strong>Pan:</strong> Shift + MMB + arrastre.</li>
          <li><strong>Zoom (dolly):</strong> Ctrl + MMB + arrastre vertical, o rueda del ratón.</li>
          <li><strong>Encuadrar selección:</strong> tecla <strong>.</strong> del teclado numérico (Frame Selected).</li>
          <li><strong>Mover entidad:</strong> tecla <strong>G</strong> (grab) o clic en flecha/centro del gizmo o arrastrando el objeto; <strong>X</strong>/<strong>Y</strong>/<strong>Z</strong> limitan el eje; clic izquierdo confirma, clic derecho o Esc cancelan.</li>
          <li><strong>Rotar entidad:</strong> tecla <strong>R</strong> alterna el gizmo entre traslación (flechas) y rotación (anillos); arrastra un anillo para rotar alrededor de ese eje.</li>
          <li><strong>Precisión:</strong> mantén Shift mientras arrastras o rotas para ajuste fino.</li>
          <li><strong>Snap a cuadrícula / ángulos:</strong> mantén Ctrl mientras arrastras (posición) o rotas (incrementos de 15°).</li>
          <li><strong>Seleccionar:</strong> clic izquierdo. Multi-selección con Ctrl + clic.</li>
          <li><strong>Propiedades de entidad:</strong> doble clic izquierdo sobre la entidad seleccionada.</li>
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
