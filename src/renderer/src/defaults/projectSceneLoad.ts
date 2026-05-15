/** Escena inicial del motor al abrir un `.save` (sin plantilla por defecto). */
export function setSceneCommandForSavedProject(projectType?: string): string {
	if (projectType === '3D') return 'empty';
	if (projectType === '2D') return '2D';
	return projectType ?? '2D';
}
