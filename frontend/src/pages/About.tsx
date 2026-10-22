import { useAsync } from '../lib/useAsync';
import {
  getAdmin,
  getPendingAdmin,
  getContractId,
  getNetwork,
} from '../lib/contract';

export default function About() {
  const admin = useAsync(() => getAdmin(), []);
  const pendingAdmin = useAsync(() => getPendingAdmin(), []);

  const contractId = getContractId();
  const network = getNetwork();

  return (
    <div className="page about-page">
      <h1>About</h1>
      <p className="about-intro">
        DepRadar is a transparent claims and refunds contract. Below you can see
        who reviews claims and the rules that govern claims and refunds.
      </p>

      <section className="about-section" aria-labelledby="contract-heading">
        <h2 id="contract-heading">Contract</h2>
        <dl className="about-details">
          <dt>Contract ID</dt>
          <dd data-testid="contract-id">{contractId ?? 'Unknown'}</dd>
          <dt>Network</dt>
          <dd data-testid="network">{network ?? 'Unknown'}</dd>
        </dl>
      </section>

      <section className="about-section" aria-labelledby="admin-heading">
        <h2 id="admin-heading">Administration</h2>
        {admin.loading && <p>Loading admin…</p>}
        {admin.error && (
          <p className="error" role="alert">
            Failed to load admin: {admin.error.message}
          </p>
        )}
        {!admin.loading && !admin.error && (
          <dl className="about-details">
            <dt>Current admin</dt>
            <dd data-testid="current-admin">
              {admin.data ?? 'Not set'}
            </dd>
            <dt>Pending admin</dt>
            <dd data-testid="pending-admin">
              {pendingAdmin.loading
                ? 'Loading…'
                : pendingAdmin.error
                  ? 'Unavailable'
                  : (pendingAdmin.data ?? 'None')}
            </dd>
          </dl>
        )}
      </section>

      <section className="about-section" aria-labelledby="rules-heading">
        <h2 id="rules-heading">Rules</h2>

        <h3>Claims</h3>
        <ul>
          <li>Claims are reviewed by the admin.</li>
          <li>Accepted claims are paid out.</li>
          <li>Rejected claims are not paid.</li>
        </ul>

        <h3>Refunds</h3>
        <ul>
          <li>
            The creator can cancel the contract after the deadline if there are
            no pending claims.
          </li>
          <li>
            The creator can also cancel after the 7-day review window has
            passed.
          </li>
        </ul>
      </section>
    </div>
  );
}