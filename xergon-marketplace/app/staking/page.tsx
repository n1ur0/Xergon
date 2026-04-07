import { Metadata } from 'next';
import StakingDashboard from '@/components/staking/StakingDashboard';

export const metadata: Metadata = {
  title: 'Staking | Xergon',
  description: 'Stake ERG to earn rewards and boost your provider reputation',
};

export default function Page() {
  return <StakingDashboard />;
}
