# Testnet Deployment Checklist

## Prerequisites

- [ ] Ergo testnet node accessible (http://192.168.1.75:9052)
- [ ] Deployer address funded with ≥10 ERG on testnet
- [ ] Ergo compiler tools installed OR Docker available
- [ ] Committee member addresses prepared (3-5 members)
- [ ] Initial voter list prepared (5-10 voters)

---

## Phase 1: Environment Setup

### Option A: Local Installation
- [ ] Install Ergo compiler: `https://github.com/ergoplatform/ergo-appkit`
- [ ] Install Ergo wallet CLI
- [ ] Verify: `ergo-compiler --version`
- [ ] Verify: `ergo-wallet --version`

### Option B: Docker (Recommended)
- [ ] Docker installed and running
- [ ] Run: `./docker-ergo-env.sh`
- [ ] Verify container is running

---

## Phase 2: Compile Contracts

Run: `./deploy-testnet.sh compile-only`

- [ ] voter_registry.ergo → voter_registry.ergotree
- [ ] governance_proposal_v2.ergo → governance_proposal_v2.ergotree
- [ ] user_staking.ergo → user_staking.ergotree
- [ ] provider_box.ergo → provider_box.ergotree
- [ ] treasury.ergo → treasury.ergotree
- [ ] provider_slashing.ergo → provider_slashing.ergotree
- [ ] usage_proof.ergo → usage_proof.ergotree

**Expected output:** All contracts compile without errors

---

## Phase 3: Deploy Voter Registry

**Before deployment:** Update `deploy-testnet.sh` with:
- Committee member addresses (3-5)
- Initial voter list (5-10)
- Update threshold (e.g., 2 for 3-of-5)

Run: `./deploy-testnet.sh deploy`

- [ ] Voter Registry deployed
- [ ] NFT ID recorded: `contracts/deployed_registry_nft.txt`
- [ ] Transaction confirmed on testnet

**Verify:**
```bash
curl http://192.168.1.75:9052/boxes/filter \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{"condition": "contains(boxes.tokens, {\"id\":\"'$(cat contracts/deployed_registry_nft.txt)'\"})"}'
```

---

## Phase 4: Deploy Governance v2

**Before deployment:** Update script with Voter Registry NFT ID

- [ ] Governance v2 deployed
- [ ] NFT ID recorded: `contracts/deployed_governance_nft.txt`
- [ ] Transaction confirmed on testnet

**Verify:**
```bash
curl http://192.168.1.75:9052/boxes/filter \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{"condition": "contains(boxes.tokens, {\"id\":\"'$(cat contracts/deployed_governance_nft.txt)'\"})"}'
```

---

## Phase 5: Deploy Other Contracts

- [ ] User Staking contract template compiled
- [ ] Provider Box contract template compiled
- [ ] Treasury contract deployed
- [ ] Provider Slashing contract deployed
- [ ] Usage Proof contract compiled

---

## Phase 6: Functional Testing

### Test 1: Voter Registry
- [ ] Query registry: Can read authorized voters
- [ ] Committee update: Can update voter list with threshold signatures
- [ ] Unauthorized update: Cannot update without sufficient signatures

### Test 2: Governance v2
- [ ] Authorized voter can create proposal
- [ ] Authorized voter can vote on proposal
- [ ] Authorized voter can execute proposal (after threshold met)
- [ ] Unauthorized voter CANNOT create/vote (should fail)
- [ ] Governance NFT preserved in all transactions

### Test 3: Provider Registration
- [ ] Provider can register with staking box
- [ ] Provider box created with correct parameters
- [ ] Provider can update models
- [ ] Provider can deregister (after unstake period)

### Test 4: User Staking
- [ ] User can create staking box
- [ ] Staking box has correct ERG amount
- [ ] User can unstake after waiting period
- [ ] Staking NFT preserved

### Test 5: Treasury
- [ ] Treasury can receive funds
- [ ] Deployer can spend from treasury
- [ ] NFT preserved in treasury transactions

### Test 6: Provider Slashing
- [ ] Provider can stake with slashing box
- [ ] Slashing box has correct parameters
- [ ] Provider can top-up stake
- [ ] Slashing works when proof submitted

---

## Phase 7: Security Verification

### Critical Tests
- [ ] Unauthorized voter cannot create proposal (on-chain check)
- [ ] Unauthorized voter cannot vote (on-chain check)
- [ ] Governance v2 requires voter registry data input
- [ ] Voter registry prevents unauthorized updates

### Medium Tests
- [ ] Single-key treasury can spend (expected behavior)
- [ ] Provider can rotate keys (if implemented)
- [ ] Rate limiting works (if implemented)

---

## Phase 8: Integration Tests

Run Xergon agent and relay:

```bash
cd xergon-agent
cargo run --release

cd ../xergon-relay
cargo run --release

cd ../xergon-marketplace
npm run dev
```

- [ ] Agent connects to Ergo node
- [ ] Agent detects deployed contracts
- [ ] Relay routes requests correctly
- [ ] Marketplace displays contracts
- [ ] End-to-end inference flow works

---

## Phase 9: Documentation

- [ ] Update deployment addresses in README.md
- [ ] Record all NFT IDs in deployment log
- [ ] Document committee member addresses
- [ ] Create voter onboarding guide
- [ ] Update CRITICAL_FIXES_IMPLEMENTATION.md with results

---

## Phase 10: Production Readiness Review

- [ ] All tests passing
- [ ] Security audit recommendations addressed
- [ ] Multi-sig treasury planned (if not implemented)
- [ ] Governance v2 tested thoroughly
- [ ] Performance benchmarks met
- [ ] Monitoring/alerting setup

**Decision:** Proceed to mainnet? [ ] YES [ ] NO

---

## Common Issues & Solutions

### Issue: Compilation errors
**Solution:** Check Ergo syntax, ensure all registers defined

### Issue: Insufficient funds
**Solution:** Fund deployer address with more testnet ERG

### Issue: Transaction fails
**Solution:** Check box value, ensure minimum ERG for script

### Issue: Voter registry not found
**Solution:** Verify NFT ID matches deployed contract

### Issue: Unauthorized access works
**Solution:** Check voter registry integration in governance v2

---

## Deployment Log Template

```
Date: _______________
Deployer: _______________
Node URL: _______________

Voter Registry NFT: _______________
Governance v2 NFT: _______________
Treasury Box: _______________
Provider Slashing: _______________

Committee Members:
1. _______________
2. _______________
3. _______________

Initial Voters:
1. _______________
2. _______________
3. _______________

Test Results:
- Voter Registry: [ ] PASS [ ] FAIL
- Governance v2: [ ] PASS [ ] FAIL
- Provider Box: [ ] PASS [ ] FAIL
- User Staking: [ ] PASS [ ] FAIL
- Treasury: [ ] PASS [ ] FAIL
- Slashing: [ ] PASS [ ] FAIL

Notes:
_________________________________
_________________________________
```

---

## Next Steps After Testnet

1. **Monitor for 2-4 weeks**
   - Track contract interactions
   - Monitor for bugs/exploits
   - Gather community feedback

2. **Prepare for mainnet**
   - Implement multi-sig treasury
   - Finalize voter registry
   - Security audit review

3. **Mainnet deployment**
   - Repeat deployment process on mainnet
   - Fund with mainnet ERG
   - Announce to community

4. **Post-deployment**
   - Continuous monitoring
   - Regular security reviews
   - Governance process activation

---

**Last Updated:** April 12, 2026  
**Version:** 1.0
