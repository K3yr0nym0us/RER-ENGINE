export interface AssetGroupConfig {
  openDialog:  () => Promise<string | null>
  loadCmd:     string
  dupCmd:      string
  addBtnLabel: string
  emptyText:   string
}

interface Props {
  config: AssetGroupConfig
}

export function CharactersAccordion({ config: _config }: Props) {
  // Este componente es ahora solo un placeholder/contenedor
  // La lógica de crear personajes está en BtnCreateCharacter
  // Las acciones de duplicar/eliminar se han movido a PropertiesAccordion
  return <div />
}

export default CharactersAccordion
