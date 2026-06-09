import { useState, type ComponentProps, type ReactNode } from 'react'
import { Accordion } from 'react-bootstrap'

type AccordionSelectKey = Parameters<NonNullable<ComponentProps<typeof Accordion>['onSelect']>>[0]

interface SidebarSubAccordionProps {
	children: ReactNode
	className?: string
}

/** Grupo de sub-accordions dentro de un accordion principal: solo uno abierto a la vez. */
export default function SidebarSubAccordion({ children, className }: SidebarSubAccordionProps) {
	const [activeKey, setActiveKey] = useState<string | null>(null)

	return (
		<Accordion
			className={['sidebar-accordion', 'sidebar-accordion-sub', className].filter(Boolean).join(' ')}
			activeKey={activeKey ?? undefined}
			onSelect={(next: AccordionSelectKey) => {
				setActiveKey(typeof next === 'string' ? next : null)
			}}
		>
			{children}
		</Accordion>
	)
}
