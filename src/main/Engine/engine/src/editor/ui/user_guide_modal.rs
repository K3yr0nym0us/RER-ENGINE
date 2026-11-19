//! Contenido de la ventana nativa de guía de usuario.
//!
//! Sustituye al componente React `UserGuide` (contenido estático). Se muestra en
//! una ventana ancha con scroll; se cierra con la X, igual que la modal Electron.

use egui::RichText;

use crate::editor::app::EditorApp;
use crate::editor::theme;

/// Texto con el mismo diseño que las listas de la guía del renderer.
fn line(ui: &mut egui::Ui, prefix: &str, parts: &[(&str, bool)]) {
    ui.horizontal_wrapped(|ui| {
        if !prefix.is_empty() {
            ui.label(RichText::new(prefix).size(13.0).color(theme::TEXT_MUTED));
        }
        for (text, bold) in parts {
            let text = RichText::new(*text).size(13.0).color(theme::TEXT_MUTED);
            ui.label(if *bold { text.strong() } else { text });
        }
    });
    ui.add_space(3.0);
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).strong().color(theme::TEXT));
    ui.add_space(4.0);
}

fn paragraph(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(13.0).color(theme::TEXT_MUTED));
    ui.add_space(6.0);
}

/// Dibuja la guía completa en la ventana modal externa.
pub(crate) fn show_in_window(ui: &mut egui::Ui, _app: &mut EditorApp) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    heading(ui, "Flujo recomendado");
                    let steps: [&[(&str, bool)]; 10] = [
                        &[("Crea o selecciona una escena en el acordeón de \"Escenas\".", false)],
                        &[("Define el mundo: tamaño, cuadrícula, etc en el accordion de \"Mundo\".", false)],
                        &[("Carga recursos al motor (sprites o modelos, sonidos y fondos en 2D) desde \"Recursos\".", false)],
                        &[("Crea entidades como: entorno, personajes y objetos en el accordion de \"Entidades\".", false)],
                        &[("Haz clic en una entidad para editarla en el accordion de \"Propiedades\".", false)],
                        &[("Convierte entidades a blueprints para reutilizarlas en el accordion de \"Propiedades\" boton \"Convertir en Blueprint\".", false)],
                        &[("Usa la herramienta de construcción rápida para usar las blueprints en el accordion de \"Herramientas\".", false)],
                        &[("Programa la escena desde ", false), ("Escenas → Programación", true), (" (nodos visuales o script Rhai de escena).", false)],
                        &[("Programa una entidad desde ", false), ("Propiedades → Programar entidad", true), (" (nodos o scripts Rhai por entidad).", false)],
                        &[("Guarda con frecuencia o activa el guardado automático.", false)],
                    ];
                    for (index, parts) in steps.iter().enumerate() {
                        line(ui, &format!("{}. ", index + 1), parts);
                    }

                    ui.add_space(10.0);
                    heading(ui, "Navegacion del editor");
                    paragraph(ui, "La barra lateral izquierda concentra la configuracion principal:");
                    line(ui, "• ", &[("Escenas:", true), (" lista de escenas del proyecto; crear, renombrar, eliminar y cambiar la escena activa. Dentro, ", false), ("Programación", true), (" abre la lógica de escena (nodos o Rhai).", false)]);
                    line(ui, "• ", &[("Mundo:", true), (" tamaño del area de trabajo, cuadrícula, gravedad, FPS objetivo y (2D) fondo del nivel; en 3D también luz direccional y sombras.", false)]);
                    line(ui, "• ", &[("Cámara:", true), (" en 2D, posición y zoom de la cámara del editor; en 3D, selector de tipo de cámara y controles del ojo (posición, FOV, frustum) cuando la cámara es play character (primera persona).", false)]);
                    line(ui, "• ", &[("Recursos:", true), (" carga y organización de assets — en 2D: sprites, sonidos y fondos; en 3D: modelos 3D y sonidos.", false)]);
                    line(ui, "• ", &[("Entidades:", true), (" creación de entorno, personajes y objetos a partir de los recursos cargados.", false)]);
                    line(ui, "• ", &[("Herramientas:", true), (" en 2D, dibujar colisionadores, áreas de ejecución y construcción rápida con blueprints; en 3D, construcción rápida con blueprints.", false)]);
                    line(ui, "• ", &[("Controles:", true), (" elige un personaje y configura teclas, mouse o mandos con scripts Rhai.", false)]);
                    line(ui, "• ", &[("Propiedades:", true), (" aparece al seleccionar una entidad; nombre, transform, física, animaciones, ", false), ("Programar entidad", true), (" (nodos o scripts Rhai) y acciones (eliminar, blueprint, etc.).", false)]);

                    ui.add_space(10.0);
                    heading(ui, "Viewport 3D");
                    paragraph(ui, "En proyectos 3D, la navegación y el movimiento de entidades siguen el esquema estándar de orbit/pan/zoom:");
                    line(ui, "• ", &[("Órbita:", true), (" botón central del ratón (MMB) + arrastre.", false)]);
                    line(ui, "• ", &[("Pan:", true), (" Shift + MMB + arrastre.", false)]);
                    line(ui, "• ", &[("Zoom (dolly):", true), (" Ctrl + MMB + arrastre vertical, o rueda del ratón.", false)]);
                    line(ui, "• ", &[("Encuadrar selección:", true), (" tecla ", false), (".", true), (" del teclado numérico (Frame Selected).", false)]);
                    line(ui, "• ", &[("Mover entidad:", true), (" tecla ", false), ("G", true), (" (grab) o clic en flecha/centro del gizmo o arrastrando el objeto; ", false), ("X", true), ("/", false), ("Y", true), ("/", false), ("Z", true), (" limitan el eje; clic izquierdo confirma, clic derecho o Esc cancelan.", false)]);
                    line(ui, "• ", &[("Rotar entidad:", true), (" tecla ", false), ("R", true), (" alterna el gizmo entre traslación (flechas) y rotación (anillos); arrastra un anillo para rotar alrededor de ese eje.", false)]);
                    line(ui, "• ", &[("Precisión:", true), (" mantén Shift mientras arrastras o rotas para ajuste fino.", false)]);
                    line(ui, "• ", &[("Snap a cuadrícula / ángulos:", true), (" mantén Ctrl mientras arrastras (posición) o rotas (incrementos de 15°).", false)]);
                    line(ui, "• ", &[("Seleccionar:", true), (" clic izquierdo. Multi-selección con Ctrl + clic.", false)]);
                    line(ui, "• ", &[("Propiedades de entidad:", true), (" doble clic izquierdo sobre la entidad seleccionada.", false)]);

                    ui.add_space(10.0);
                    heading(ui, "Guardado y proyecto");
                    paragraph(ui, "Usa el boton superior derecho para guardar el proyecto. Una vez guardado se activará el guardado automatico. Cada escena conserva sus propias entidades y configuración de mundo.");
                });
        });
}
