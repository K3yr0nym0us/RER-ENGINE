import { isValidElement, useId, type ComponentProps, type ReactElement } from 'react';

import { OverlayTrigger, Tooltip } from 'react-bootstrap';

type Placement = NonNullable<ComponentProps<typeof OverlayTrigger>['placement']>;

interface AppTooltipProps {
  content?: string | null;
  place?: Placement;
  variant?: string;
  offset?: number;
  delayShow?: number;
  delayHide?: number;
  float?: boolean;
  tooltipClassName?: string;
  children: ReactElement;
}

export function AppTooltip({
  content,
  place = 'top',
  variant,
  offset,
  delayShow,
  delayHide,
  tooltipClassName,
  children,
}: AppTooltipProps) {
  const tooltipId = useId().replace(/:/g, '');

  if (!content || !isValidElement(children)) {
    return children;
  }

  const delay = (delayShow !== undefined || delayHide !== undefined)
    ? { show: delayShow ?? 0, hide: delayHide ?? 0 }
    : undefined;

  const className = [tooltipClassName, variant ? `app-tooltip--${variant}` : null]
    .filter(Boolean)
    .join(' ');

  const popperConfig = offset !== undefined
    ? {
        modifiers: [
          {
            name: 'offset',
            options: {
              offset: [0, offset],
            },
          },
        ],
      }
    : undefined;

  return (
    <OverlayTrigger
      trigger={['hover', 'focus']}
      placement={place}
      delay={delay}
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