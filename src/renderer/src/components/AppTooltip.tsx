import { isValidElement, useId, useMemo, type ComponentProps, type ReactElement, type ReactNode } from 'react';

import { OverlayTrigger, Tooltip } from 'react-bootstrap';

type Placement = NonNullable<ComponentProps<typeof OverlayTrigger>['placement']>;

interface AppTooltipProps {
  content?: ReactNode;
  place?: Placement;
  variant?: string;
  offset?: number;
  delayShow?: number;
  delayHide?: number;
  float?: boolean;
  tooltipClassName?: string;
  disableFlip?: boolean;
  children: ReactElement;
}

function tooltipContainer(): HTMLElement {
  return document.body;
}

function buildPopperConfig(offset?: number, disableFlip?: boolean) {
  const modifiers: Record<string, unknown>[] = [
    {
      name: 'flip',
      enabled: !disableFlip,
      options: disableFlip
        ? undefined
        : {
            fallbackPlacements: [
              'top',
              'top-start',
              'top-end',
              'bottom',
              'bottom-start',
              'bottom-end',
              'left',
              'right',
            ],
          },
    },
    {
      name: 'preventOverflow',
      options: {
        padding: 8,
        rootBoundary: 'viewport',
        altAxis: true,
        tether: true,
      },
    },
  ];

  if (offset !== undefined) {
    modifiers.unshift({
      name: 'offset',
      options: {
        offset: [0, offset],
      },
    });
  }

  return { strategy: 'fixed' as const, modifiers };
}

export function AppTooltip({
  content,
  place = 'top',
  variant,
  offset,
  delayShow,
  delayHide,
  tooltipClassName,
  disableFlip,
  children,
}: AppTooltipProps) {
  const tooltipId = useId().replace(/:/g, '');

  const popperConfig = useMemo(
    () => buildPopperConfig(offset, disableFlip),
    [offset, disableFlip],
  );

  if (!content || !isValidElement(children)) {
    return children;
  }

  const delay = (delayShow !== undefined || delayHide !== undefined)
    ? { show: delayShow ?? 0, hide: delayHide ?? 0 }
    : undefined;

  const className = [tooltipClassName, variant ? `app-tooltip--${variant}` : null]
    .filter(Boolean)
    .join(' ');

  return (
    <OverlayTrigger
      trigger={['hover', 'focus']}
      placement={place}
      delay={delay}
      container={tooltipContainer}
      popperConfig={popperConfig}
      overlay={
        <Tooltip id={tooltipId} className={className || undefined}>
          {content}
        </Tooltip>
      }
    >
      {children}
    </OverlayTrigger>
  );
}

export default AppTooltip;
