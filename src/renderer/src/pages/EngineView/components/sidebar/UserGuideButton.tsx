import { InfoCircle } from 'react-bootstrap-icons';
import { UserGuide } from './UserGuide';
import { AppTooltip } from '@components';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';


export function UserGuideButton() {
  const { t } = useTraslate()
  const { openModal } = useModal()

  const openUserGuide = () => {
    openModal({
      title: t('User guide'),
      body: <UserGuide />,
      size: 'lg',
    })
  }

  return (
    <AppTooltip content={t('Open user guide')} place="top">
      <button
        type="button"
        className="btn btn-outline-info btn-sm w-100 d-flex align-items-center justify-content-center gap-2 fw-semibold"
        onClick={openUserGuide}
      >
        <InfoCircle size={16} />
        <span>{t('User guide')}</span>
      </button>
    </AppTooltip>
  )
}

export default UserGuideButton