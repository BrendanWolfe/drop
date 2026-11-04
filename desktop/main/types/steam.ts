export type SteamLaunch = {
  executable: string;
  arguments: string[];
  workingDirectory: string;
  launchId: string;
  configuration: string;
};
export type SteamContext = {
  supported: boolean;
  profiles: Array<{
    id: string;
    accountId: string;
    name: string;
    steamRoot: string;
  }>;
  selectedProfile: string | null;
  running: boolean;
  options: Array<{
    id: string;
    name: string;
    launch: SteamLaunch | null;
    reason: string | null;
  }>;
  registration: null | {
    key: string;
    profile: string;
    name: string;
    launchId: string;
    gameId64: string;
    preferSteam: boolean;
    state: string;
  };
};
