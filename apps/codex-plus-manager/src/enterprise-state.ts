export type EnterpriseCommandOutcome = {
  outcome: string;
  retryable?: boolean;
};

export function shouldPreserveAuthenticatedSnapshot(
  result: EnterpriseCommandOutcome,
  currentState: string | null | undefined,
  preserveRequested: boolean,
): boolean {
  return result.outcome !== "ok"
    && result.retryable === true
    && currentState === "authenticated"
    && preserveRequested;
}
