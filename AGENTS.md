# AGENTS.md - Instrucciones para agentes de IA

## Proyecto
- **Nombre**: RER-ENGINE — React + Electron + Rust Game Engine
- **Tecnologías**: Electron, React, TypeScript, Rust, Bootstrap
- ** Gestor de paquetes**: yarn (OBLIGATORIO)

## Reglas generales

### Estilo de código
- Busca la solución menos invasiva primero.
- Sugiere código, no cambies directamente a menos que el usuario pida lo contrario.
- Usa el código existente como referencia: sigue el mismo estilo y estructura.
- DRY (Don't Repeat Yourself): evita duplicación, reutiliza componentes/hooks existentes.
- Jamás pongas tareas asíncronas dentro de componentes: siempre crea un hook.

### Arquitectura
- Motor (Rust): separar lógica 2D de 3D y de renderizado.
- Frontend: Español o inglés según contexto y audiencia, pero mantener consistencia.
- Motor (Rust): toda estructura en inglés, solo logs en español.

### Antes de committing cambios grandes
- Ejecutar en el directorio del renderer: `npx -y react-doctor@latest`
- Verificar errores de dependencias o problemas con hooks.
- Si hay problemas, corregirlos antes de continuar.

### Dudas
- Consultar al desarrollador directamente. No buscar en otros directorios.