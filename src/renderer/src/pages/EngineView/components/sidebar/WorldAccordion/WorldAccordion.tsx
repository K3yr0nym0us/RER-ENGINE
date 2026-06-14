import { Accordion } from 'react-bootstrap'
import { Globe2 } from 'react-bootstrap-icons'

import { useTraslate } from '@hooks'
import type { ProjectType } from '@shared-types'
import SidebarSubAccordion from '../SidebarSubAccordion'
import WorldBackgroundAccordion from './WorldBackgroundAccordion'
import WorldGridAccordion from './WorldGridAccordion'
import WorldLightingAccordion from './WorldLightingAccordion'
import WorldPerformanceAccordion from './WorldPerformanceAccordion'
import WorldPhysicsAccordion from './WorldPhysicsAccordion'
import WorldTexturesAccordion from './WorldTexturesAccordion'
import WorldWorkspaceAccordion from './WorldWorkspaceAccordion'

export function WorldAccordion({ projectType = '2D' }: { projectType?: ProjectType }) {
	const { t } = useTraslate()
	const is3dProject = projectType === '3D'

	return (
		<Accordion.Item eventKey="mundo">
			<Accordion.Header>
				<Globe2 className="me-2" />
				{t('World')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-1">
				<SidebarSubAccordion>
					<WorldWorkspaceAccordion projectType={projectType} />
					{!is3dProject && <WorldBackgroundAccordion />}
					<WorldGridAccordion projectType={projectType} />
					{is3dProject && <WorldLightingAccordion />}
					{is3dProject && <WorldTexturesAccordion />}
					<WorldPhysicsAccordion />
					<WorldPerformanceAccordion />
				</SidebarSubAccordion>
			</Accordion.Body>
		</Accordion.Item>
	)
}

export default WorldAccordion
