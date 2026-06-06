import { useMemo } from 'react'

import { Accordion } from 'react-bootstrap'
import { Copy } from 'react-bootstrap-icons'

import type { Blueprint3D, Entity3D, VisualGraphContext } from '@shared-types'

import { AppTooltip } from '@components'
import { useTraslate } from '@hooks'
import {
  type ContextVariable,
  type SceneBlueprintGroup,
  type SceneEntityRow,
  buildAnimationAccordionGroups,
  buildScenePanelStructure,
  getContextVariables,
  groupEntityVariablesForAccordion,
} from '../contextVariables'

interface Props {
  context: VisualGraphContext
  sceneEntities?: Entity3D[]
  blueprints?: Blueprint3D[]
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

function CopyIconButton({
  tooltipKey,
  value,
  onCopied,
  t,
}: {
  tooltipKey: string
  value: string
  onCopied?: () => void
  t: (key: string) => string
}) {
  return (
    <AppTooltip content={t(tooltipKey)} place="top" delayShow={200}>
      <button
        type="button"
        className="visual-scripting-copy-btn"
        aria-label={t(tooltipKey)}
        onClick={(event) => {
          event.stopPropagation()
          void navigator.clipboard.writeText(value)
          onCopied?.()
        }}
      >
        <Copy size={14} className="text-success" />
      </button>
    </AppTooltip>
  )
}

function VariableItem({
  variable,
  onCopy,
  t,
}: {
  variable: ContextVariable
  onCopy: (variable: ContextVariable) => void
  t: (key: string) => string
}) {
  const snippetText = variable.kind === 'animation'
    ? variable.label
    : variable.rhaiSnippet

  const button = (
    <button
      type="button"
      className="visual-scripting-var-item"
      onClick={() => onCopy(variable)}
    >
      <span className="visual-scripting-var-label text-truncate">
        {variable.labelKey ? t(variable.labelKey) : variable.label}
      </span>
      <span className="visual-scripting-var-snippet text-truncate">{snippetText}</span>
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
  onCopy: (variable: ContextVariable) => void
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

function EntityRowList({
  rows,
  copyTooltipKey,
  t,
}: {
  rows: SceneEntityRow[]
  copyTooltipKey: string
  t: (key: string) => string
}) {
  if (rows.length === 0) return null

  return (
    <ul className="list-unstyled mb-0 d-flex flex-column gap-1">
      {rows.map((row) => (
        <li key={row.id} className="visual-scripting-entity-row">
          <span className="visual-scripting-entity-row-label text-truncate">{row.name}</span>
          <CopyIconButton tooltipKey={copyTooltipKey} value={String(row.id)} t={t} />
        </li>
      ))}
    </ul>
  )
}

function AnimationRowList({
  items,
  entityId,
  onPickAnimation,
  t,
}: {
  items: ContextVariable[]
  entityId: number
  onPickAnimation?: (entityId: number, animationName: string) => void
  t: (key: string) => string
}) {
  if (items.length === 0) return null

  return (
    <ul className="list-unstyled mb-0 d-flex flex-column gap-1">
      {items.map((variable) => (
        <li key={variable.id} className="visual-scripting-entity-row">
          <span className="visual-scripting-entity-row-label text-truncate">{variable.label}</span>
          <CopyIconButton
            tooltipKey="Copy ID"
            value={String(entityId)}
            t={t}
            onCopied={() => {
              onPickAnimation?.(entityId, variable.label)
            }}
          />
        </li>
      ))}
    </ul>
  )
}

function BlueprintGroupAccordion({
  group,
  t,
}: {
  group: SceneBlueprintGroup
  t: (key: string) => string
}) {
  return (
    <Accordion.Item eventKey={`env-bp-${group.blueprintId}`}>
      <Accordion.Header>
        <div className="visual-scripting-accordion-header-row">
          <span className="text-truncate">{group.baseName}</span>
          <CopyIconButton tooltipKey="Copy ID" value={String(group.baseEntityId)} t={t} />
        </div>
      </Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <EntityRowList rows={group.instances} copyTooltipKey="Copy ID" t={t} />
      </Accordion.Body>
    </Accordion.Item>
  )
}

function EnvironmentAccordionSection({
  baseEnvironment,
  standalone,
  blueprintGroups,
  t,
}: {
  baseEnvironment: SceneEntityRow[]
  standalone: SceneEntityRow[]
  blueprintGroups: SceneBlueprintGroup[]
  t: (key: string) => string
}) {
  const hasBase = baseEnvironment.length > 0
  const hasStandalone = standalone.length > 0
  const hasBlueprints = blueprintGroups.length > 0
  if (!hasBase && !hasStandalone && !hasBlueprints) return null

  const defaultNestedKey = hasBase
    ? 'env-base'
    : (hasBlueprints ? `env-bp-${blueprintGroups[0]?.blueprintId}` : undefined)

  return (
    <Accordion.Item eventKey="entity-environment">
      <Accordion.Header>{t('Environment')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        {(hasBase || hasBlueprints) && (
          <Accordion
            className="sidebar-accordion visual-scripting-animations-nested"
            defaultActiveKey={defaultNestedKey}
          >
            {hasBase && (
              <Accordion.Item eventKey="env-base">
                <Accordion.Header className="small">{t('Base environment')}</Accordion.Header>
                <Accordion.Body className="py-2 px-2">
                  <EntityRowList rows={baseEnvironment} copyTooltipKey="Copy ID" t={t} />
                </Accordion.Body>
              </Accordion.Item>
            )}
            {blueprintGroups.map((group) => (
              <BlueprintGroupAccordion key={group.blueprintId} group={group} t={t} />
            ))}
          </Accordion>
        )}

        {hasStandalone && (
          <div className={hasBase || hasBlueprints ? 'mt-2' : undefined}>
            <EntityRowList rows={standalone} copyTooltipKey="Copy ID" t={t} />
          </div>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}

function PlayerAccordionSection({
  player,
  onPickAnimation,
  t,
}: {
  player: NonNullable<ReturnType<typeof buildScenePanelStructure>['player']>
  onPickAnimation?: (entityId: number, animationName: string) => void
  t: (key: string) => string
}) {
  return (
    <Accordion.Item eventKey="entity-player">
      <Accordion.Header>
        <div className="visual-scripting-accordion-header-row">
          <span className="text-truncate">{t('Player')}</span>
          <CopyIconButton tooltipKey="Copy ID" value={String(player.entityId)} t={t} />
        </div>
      </Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        {player.animations.length > 0 ? (
          <Accordion className="sidebar-accordion visual-scripting-animations-nested" defaultActiveKey="player-animations">
            <Accordion.Item eventKey="player-animations">
              <Accordion.Header className="small">{t('Animations')}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <AnimationRowList
                  items={player.animations}
                  entityId={player.entityId}
                  onPickAnimation={onPickAnimation}
                  t={t}
                />
              </Accordion.Body>
            </Accordion.Item>
          </Accordion>
        ) : (
          <p className="small visual-scripting-variables-hint mb-0">{t('No animations in scene')}</p>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}

export function VisualScriptVariablesPanel({
  context,
  sceneEntities,
  blueprints,
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
  const scenePanel = useMemo(
    () => (context === 'scene'
      ? buildScenePanelStructure(variables, sceneEntities, blueprints)
      : null),
    [context, variables, sceneEntities, blueprints],
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
    : (scenePanel?.globals.length ? 'scene-globals' : 'entity-environment')

  const copySnippet = (variable: ContextVariable) => {
    void navigator.clipboard.writeText(variable.rhaiSnippet)
  }

  const title = context === 'entity' ? t('Entity variables') : t('Scene references')

  const hasSceneContent = context === 'scene' && scenePanel && (
    scenePanel.globals.length > 0
    || scenePanel.player != null
    || scenePanel.characters.length > 0
    || scenePanel.objects.length > 0
    || scenePanel.environment.baseEnvironment.length > 0
    || scenePanel.environment.standalone.length > 0
    || scenePanel.environment.blueprintGroups.length > 0
  )

  return (
    <div
      className="border-end border-secondary p-2 bg-dark text-light visual-scripting-variables visual-scripting-inspector flex-shrink-0 overflow-auto"
      style={{ width: 240, maxHeight: '100%' }}
    >
      <div className="visual-scripting-variables-title mb-2">{title}</div>
      {context === 'scene' && !hasSceneContent && (
        <p className="small visual-scripting-variables-hint mb-0">{t('No variables available')}</p>
      )}

      {context === 'scene' && scenePanel && hasSceneContent && (
        <Accordion className="sidebar-accordion mb-2" defaultActiveKey={defaultActiveKey}>
          {scenePanel.globals.length > 0 && (
            <Accordion.Item eventKey="scene-globals">
              <Accordion.Header>{t('Scene logic')}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <VariableList items={scenePanel.globals} onCopy={copySnippet} t={t} />
              </Accordion.Body>
            </Accordion.Item>
          )}

          {(scenePanel.environment.baseEnvironment.length > 0
            || scenePanel.environment.standalone.length > 0
            || scenePanel.environment.blueprintGroups.length > 0) && (
            <EnvironmentAccordionSection
              baseEnvironment={scenePanel.environment.baseEnvironment}
              standalone={scenePanel.environment.standalone}
              blueprintGroups={scenePanel.environment.blueprintGroups}
              t={t}
            />
          )}

          {scenePanel.characters.length > 0 && (
            <Accordion.Item eventKey="entity-character">
              <Accordion.Header>{t('Characters')}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <VariableList items={scenePanel.characters} onCopy={copySnippet} t={t} />
              </Accordion.Body>
            </Accordion.Item>
          )}

          {scenePanel.player && (
            <PlayerAccordionSection
              player={scenePanel.player}
              onPickAnimation={onPickAnimation}
              t={t}
            />
          )}

          {scenePanel.objects.length > 0 && (
            <Accordion.Item eventKey="entity-object">
              <Accordion.Header>{t('Objects')}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <VariableList items={scenePanel.objects} onCopy={copySnippet} t={t} />
              </Accordion.Body>
            </Accordion.Item>
          )}
        </Accordion>
      )}

      {context === 'entity' && entityGroups.length > 0 && (
        <Accordion className="sidebar-accordion" defaultActiveKey={defaultActiveKey}>
          {entityGroups.map((group) => (
            <Accordion.Item key={group.eventKey} eventKey={group.eventKey}>
              <Accordion.Header>{t(group.labelKey)}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <VariableList
                  items={group.items}
                  onCopy={(variable) => {
                    copySnippet(variable)
                    if (onPickAnimation && group.eventKey === 'entity-var-animations' && variable.kind === 'animation') {
                      if (entityId != null) {
                        onPickAnimation(entityId, variable.label)
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
