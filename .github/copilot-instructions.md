Instrucciones y informacion sobre el proyecto para la IA de GitHub Copilot.

- Uso de "yarn" para manejo y control de dependencias OBLIGATORIO.
- App de escritorio con Electron, React y TypeScript (Motor en Rust).

Por favor buscar la solucion menos invasiva y trata sugerir codigo no cambiar directamente.
Siempre usa como ejemplo el codigo existente en el proyecto, sobre todo en creacion de hooks y componentes, sigue el mismo estilo y estructura y jamas pongas tareas asincronas dentro de un componente, siempre crea un hook para eso.

La configuracion del motor y toda su estrutura debe estar en ingles y solo los logs en español, el front puede estar en español o ingles dependiendo del contexto y la audiencia, pero siempre manteniendo consistencia dentro de la aplicación.

Despues de cada cambio grande e importante que afecte al front usa npx -y react-doctor@latest en el directorio de la app correspondiente para verificar que no hay errores de dependencias o problemas con los hooks, si algun hook o componente de los que creaste o editaste tiene problemas de dependencias o hooks, corrige esos problemas antes de seguir con el siguiente cambio.

Código DRY (Don't Repeat Yourself), evita la duplicación de código y busca siempre la forma de reutilizar componentes, hooks o funciones existentes antes de crear nuevos.

En el motor hay que dividir los archivos de la logica 2D de la 3D, y de la logica de renderizado, para mantener el codigo organizado y facil de mantener.

Cualquier duda fuera de este readme consulta al desarrollador no busques en otros directorios.