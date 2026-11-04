/** A failed Steam request must propagate, never imply permission to launch directly. */
export async function requestPreferredSteamLaunch(
  id: string,
  direct: boolean,
  request: (id: string) => Promise<boolean>,
): Promise<boolean> {
  if (direct) return false;
  return await request(id);
}
