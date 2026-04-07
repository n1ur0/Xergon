import { Metadata } from 'next';
import ReputationDashboard from '@/components/reputation/ReputationDashboard';

export const metadata: Metadata = {
  title: 'Reputation | Xergon',
  description: 'Provider reputation dashboard',
};

export default function Page() {
  return <ReputationDashboard />;
}
