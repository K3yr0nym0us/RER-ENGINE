import { useTraslate } from '@hooks'

interface ReflectionTierTooltipNoteProps {
	descKey: string
}

export function ReflectionTierTooltipNote({ descKey }: ReflectionTierTooltipNoteProps) {
	const { t } = useTraslate()
	return <p className="reflection-tier-tooltip-note mb-0">{t(descKey)}</p>
}
