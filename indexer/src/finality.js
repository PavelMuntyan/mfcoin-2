export const FINALITY_HEALTHY = "healthy";
export const FINALITY_DELAYED = "finality_delayed";

export function finalityStatus(bestNumber, finalizedNumber, maximumLag = 6) {
  const lag = Math.max(0, Number(bestNumber) - Number(finalizedNumber));
  return {
    bestNumber: Number(bestNumber),
    finalizedNumber: Number(finalizedNumber),
    lag,
    status: lag > maximumLag ? FINALITY_DELAYED : FINALITY_HEALTHY,
    maximumLag: Number(maximumLag)
  };
}

export function assertFinalityHealthy(bestNumber, finalizedNumber, maximumLag = 6) {
  const state = finalityStatus(bestNumber, finalizedNumber, maximumLag);
  if (state.status !== FINALITY_HEALTHY) {
    const error = new Error(`Finality delayed: best ${state.bestNumber}, finalized ${state.finalizedNumber}, lag ${state.lag}`);
    error.code = FINALITY_DELAYED;
    throw error;
  }
  return state;
}
