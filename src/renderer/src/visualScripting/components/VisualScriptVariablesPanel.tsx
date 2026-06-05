import { useMemo } from 'react'

import { Accordion } from 'react-bootstrap'

import type { Entity3D, VisualGraphContext } from '@shared-types'

import { AppTooltip } from '@components'
import { useTraslate } from '@hooks'
import {
  type ContextVariable,
  buildAnimationAccordionGroups,
  getContextVariables,
  groupEntityVariablesForAccordion,
  groupSceneVariablesForAccordion,
} from '../contextVariables'

interface Props {
  context: VisualGraphContext
  sceneEntities?: Entity3D[]
  entityId?: number
  entityName?: string
  /** Si hay un nodo Play animation seleccionado, rellena sus campos al elegir una animación. */
  onPickAnimation?: (entityId: number, animationName: string) => void
}

function VariableTooltipContent({ variable, t }: { variable: ContextVariable; t: (key: string) => string }) {
  return (
    <div className="text-start visual-scripting-var-tooltip">
      {variable.description && (
        <div>{t(variable.description)}</div>
      )}
      {variable.detail?.trim() && (
        <div className="visual-scripting-var-tooltip-detail">{variable.detail}</div>
      )}
      <div className="visual-scripting-var-tooltip-hint">{t('Click to copy snippet to clipboard')}</div>
    </div>
  )
}

function VariableItem({
  variable,
  onCopy,
  t,
}: {
  variable: ContextVariable
  onCopy: (snippet: string) => void
  t: (key: string) => string
}) {
  const button = (
    <button
      type="button"
      className="visual-scripting-var-item"
      onClick={() => onCopy(variable.rhaiSnippet)}
    >
      <span className="visual-scripting-var-label text-truncate">
        {variable.labelKey ? t(variable.labelKey) : variable.label}
      </span>
      <span className="visual-scripting-var-snippet text-truncate">{variable.rhaiSnippet}</span>
    </button>
  )

  return (
    <li>
      <AppTooltip
        content={<VariableTooltipContent variable={variable} t={t} />}
        place="right"
        delayShow={250}
      >
        {button}
      </AppTooltip>
    </li>
  )
}

function VariableList({
  items,
  onCopy,
  t,
}: {
  items: ContextVariable[]
  onCopy: (snippet: string) => void
  t: (key: string) => string
}) {
  return (
    <ul className="list-unstyled mb-0 d-flex flex-column gap-1">
      {items.map((variable) => (
        <VariableItem key={variable.id} variable={variable} onCopy={onCopy} t={t} />
      ))}
    </ul>
  )
}

function AnimationsAccordionSection({
  animationGroups,
  onCopy,
  onPickAnimation,
  t,
  rootEventKey = 'scene-animations',
}: {
  animationGroups: ReturnType<typeof buildAnimationAccordionGroups>
  onCopy: (snippet: string) => void
  onPickAnimation?: (entityId: number, animationName: string) => void
  t: (key: string) => string
  rootEventKey?: string
}) {
  if (animationGroups.length === 0) return null

  const defaultEntityKey = `anim-entity-${animationGroups[0]?.entityId}`

  return (
    <Accordion.Item eventKey={rootEventKey}>
      <Accordion.Header>{t('Animations')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <Accordion className="sidebar-accordion visual-scripting-animations-nested" defaultActiveKey={defaultEntityKey}>
          {animationGroups.map((group) => (
            <Accordion.Item key={group.entityId} eventKey={`anim-entity-${group.entityId}`}>
              <Accordion.Header className="small">{group.entityName}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <VariableList
                  items={group.items}
                  onCopy={(snippet) => {
                    onCopy(snippet)
                    if (onPickAnimation) {
                      const anim = group.items.find((item) => item.rhaiSnippet === snippet)
                      if (anim) {
                        onPickAnimation(group.entityId, anim.label)
                      }
                    }
                  }}
                  t={t}
                />
              </Accordion.Body>
            </Accordion.Item>
          ))}
        </Accordion>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export function VisualScriptVariablesPanel({
  context,
  sceneEntities,
  entityId,
  entityName,
  onPickAnimation,
}: Props) {
  const { t } = useTraslate()
  const variables = getContextVariables(context, {
    sceneEntities,
    entityId,
    entityName,
  })
  const sceneGroups = useMemo(
    () => (context === 'scene' ? groupSceneVariablesForAccordion(variables) : []),
    [context, variables],
  )
  const animationGroups = useMemo(
    () => buildAnimationAccordionGroups(sceneEntities, context === 'entity' ? { entityId } : undefined),
    [sceneEntities, context, entityId],
  )
  const entityAnimationItems = useMemo(
    () => animationGroups.flatMap((group) => group.items),
    [animationGroups],
  )
  const entityGroups = useMemo(
    () => (context === 'entity'
      ? groupEntityVariablesForAccordion(variables, entityAnimationItems)
      : []),
    [context, variables, entityAnimationItems],
  )
  const defaultActiveKey = context === 'entity'
    ? (entityGroups[0]?.eventKey ?? 'entity-var-transform')
    : (sceneGroups[0]?.eventKey ?? (animationGroups.length > 0 ? 'scene-animations' : undefined))

  const copySnippet = (snippet: string) => {
    void navigator.clipboard.writeText(snippet)
  }

  const title = context === 'entity' ? t('Entity variables') : t('Scene references')

  return (
    <div
      className="border-end border-secondary p-2 bg-dark text-light visual-scripting-variables visual-scripting-inspector flex-shrink-0 overflow-auto"
      style={{ width: 240, maxHeight: '100%' }}
    >
      <div className="visual-scripting-variables-title mb-2">{title}</div>
      {context === 'scene' && variables.length === 0 && animationGroups.length === 0 && (
        <p className="small visual-scripting-variables-hint mb-0">{t('No variables available')}</p>
      )}

      {context === 'scene' && sceneGroups.length > 0 && (
        <Accordion className="sidebar-accordion mb-2" defaultActiveKey={defaultActiveKey}>
          {sceneGroups.map((group) => (
            <Accordion.Item key={group.eventKey} eventKey={group.eventKey}>
              <Accordion.Header>{t(group.labelKey)}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <VariableList items={group.items} onCopy={copySnippet} t={t} />
              </Accordion.Body>
            </Accordion.Item>
          ))}
        </Accordion>
      )}

      {context === 'scene' && (
        animationGroups.length > 0 ? (
          <Accordion className="sidebar-accordion" defaultActiveKey="scene-animations">
            <AnimationsAccordionSection
              animationGroups={animationGroups}
              onCopy={copySnippet}
              onPickAnimation={onPickAnimation}
              t={t}
            />
          </Accordion>
        ) : (
          <p className="small visual-scripting-variables-hint mb-0">{t('No animations in scene')}</p>
        )
      )}

      {context === 'entity' && entityGroups.length > 0 && (
        <Accordion className="sidebar-accordion" defaultActiveKey={defaultActiveKey}>
          {entityGroups.map((group) => (
            <Accordion.Item key={group.eventKey} eventKey={group.eventKey}>
              <Accordion.Header>{t(group.labelKey)}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <VariableList
                  items={group.items}
                  onCopy={(snippet) => {
                    copySnippet(snippet)
                    if (onPickAnimation && group.eventKey === 'entity-var-animations') {
                      const anim = group.items.find((item) => item.rhaiSnippet === snippet && item.kind === 'animation')
                      if (anim && entityId != null) {
                        onPickAnimation(entityId, anim.label)
                      }
                    }
                  }}
                  t={t}
                />
              </Accordion.Body>
            </Accordion.Item>
          ))}
        </Accordion>
      )}

      {context === 'entity' && entityGroups.length === 0 && variables.length === 0 && (
        <p className="small visual-scripting-variables-hint mb-0">{t('No variables available')}</p>
      )}
    </div>
  )
}
