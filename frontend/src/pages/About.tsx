import { useTranslation } from 'react-i18next';
import { useAsync } from '../lib/useAsync';
import {
  getAdmin,
  getPendingAdmin,
  getContractId,
  getNetwork,
} from '../lib/contract';

export default function About() {
  const { t } = useTranslation();
  const admin = useAsync(() => getAdmin(), []);
  const pendingAdmin = useAsync(() => getPendingAdmin(), []);

  const contractId = getContractId();
  const network = getNetwork();

  return (
    <div className="page about-page">
      <h1>{t('about.title')}</h1>
      <p className="about-intro">{t('about.intro')}</p>

      <section className="about-section" aria-labelledby="contract-heading">
        <h2 id="contract-heading">{t('about.contract')}</h2>
        <dl className="about-details">
          <dt>{t('about.contractId')}</dt>
          <dd data-testid="contract-id">{contractId ?? t('about.unknown')}</dd>
          <dt>{t('about.network')}</dt>
          <dd data-testid="network">{network ?? t('about.unknown')}</dd>
        </dl>
      </section>

      <section className="about-section" aria-labelledby="admin-heading">
        <h2 id="admin-heading">{t('about.administration')}</h2>
        {admin.loading && <p>{t('about.loadingAdmin')}</p>}
        {admin.error && (
          <p className="error" role="alert">
            {t('about.failedAdmin', { message: admin.error.message })}
          </p>
        )}
        {!admin.loading && !admin.error && (
          <dl className="about-details">
            <dt>{t('about.currentAdmin')}</dt>
            <dd data-testid="current-admin">{admin.data ?? t('about.notSet')}</dd>
            <dt>{t('about.pendingAdmin')}</dt>
            <dd data-testid="pending-admin">
              {pendingAdmin.loading
                ? t('about.loading')
                : pendingAdmin.error
                  ? t('about.unavailable')
                  : (pendingAdmin.data ?? t('about.none'))}
            </dd>
          </dl>
        )}
      </section>

      <section className="about-section" aria-labelledby="rules-heading">
        <h2 id="rules-heading">{t('about.rules')}</h2>

        <h3>{t('about.claims')}</h3>
        <ul>
          <li>{t('about.claimsRule1')}</li>
          <li>{t('about.claimsRule2')}</li>
          <li>{t('about.claimsRule3')}</li>
        </ul>

        <h3>{t('about.refunds')}</h3>
        <ul>
          <li>{t('about.refundsRule1')}</li>
          <li>{t('about.refundsRule2')}</li>
        </ul>
      </section>
    </div>
  );
}
