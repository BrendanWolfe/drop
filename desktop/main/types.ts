import type { Component } from "vue";

export type NavigationItem = {
  prefix: string;
  route: string;
  label: string;
};

export type QuickActionNav = {
  icon: Component;
  notifications?: number;
  action: () => Promise<void>;
};

export type User = {
  id: string;
  username: string;
  admin: boolean;
  displayName: string;
  profilePictureObjectId: string;
};

type UmuState = "Installed" | "NotInstalled" | "NotNeeded";

export type AppState = {
  status: AppStatus;
  umuState: UmuState;
  user?: User;
};

export type Game = {
  id: string;
  type: "Game" | "Emulator" | "Dependency" | "Executor" | "Redist";
  mName: string;
  mShortDescription: string;
  mDescription: string;
  mIconObjectId: string;
  mBannerObjectId: string;
  mCoverObjectId: string;
  mImageLibraryObjectIds: string[];
  mImageCarouselObjectIds: string[];
  playtime: Array<{ seconds: number }>;
};

export type Collection = {
  id: string;
  name: string;
  isDefault: boolean;
  isTools?: boolean;
  entries: Array<{ gameId: string; game: Game }>;
};

export type GameVersion = {
  userConfiguration: {
    launchTemplate: string;
    overrideProtonPath: string;
    overrideHandler: string | undefined;
    enableUpdates: boolean;
  };
  setups: Array<{ platform: string }>;
  launches: Array<{ platform: string }>;
};

export type InstalledEmulator = {
  gameId: string;
  name: string;
  versionId: string;
  versionName: string;
  platform: string;
  installDir: string;
  installType: { type: InstalledType };
  busy: boolean;
  requiredBy: Array<{ gameId: string; name: string; versionName: string }>;
};

export type MissingEmulator = {
  gameId: string;
  versionId: string;
  platform: string;
  name: string;
  versionName: string;
};

export type InstallLocationKind = "game" | "emulator";
export type InstallLocationList = {
  directories: string[];
  defaultIndex: number | null;
};
export type InstallLocations = Record<InstallLocationKind, InstallLocationList>;

export enum AppStatus {
  NotConfigured = "NotConfigured",
  Offline = "Offline",
  SignedOut = "SignedOut",
  SignedIn = "SignedIn",
  SignedInNeedsReauth = "SignedInNeedsReauth",
  ServerUnavailable = "ServerUnavailable",
}

export type EmptyGameStatusEnum =
  | "Remote"
  | "Queued"
  | "Downloading"
  | "Validating"
  | "Extracting"
  | "Updating"
  | "Uninstalling"
  | "Running";

export enum InstalledType {
  PartiallyInstalled = "PartiallyInstalled",
  SetupRequired = "SetupRequired",
  Installed = "Installed",
}

export interface InstalledGameStatusData {
  install_type: { type: InstalledType };
  version_id: string;
  install_dir: string;
  update_available: boolean;
}

export type GameStatus =
  | {
      type: EmptyGameStatusEnum;
    }
  | ({
      type: "Installed";
    } & InstalledGameStatusData);

export type GameStatusEnum = GameStatus["type"];

export type RawGameStatus = [GameStatus | null, GameStatus | null];

export enum DownloadableType {
  Game = "Game",
  Tool = "Tool",
  DLC = "DLC",
  Mod = "Mod",
}

export type DownloadableMetadata = {
  id: string;
  version: string;
  downloadType: DownloadableType;
};

export type Settings = {
  autostart: boolean;
  maxDownloadThreads: number;
  maxDownloadSpeed: number;
  automaticClientUpdates: boolean;
  forceOffline: boolean;
};

export type LocalGame = {
  id: string;
  name: string;
  root: string;
  launcher: string | null;
  arguments: string;
  workingDirectory: string;
  platform: "Windows" | "Linux" | "macOS";
  playtimeSeconds: number;
  archive: null | {
    url: string;
    archivePath: string;
    state:
      | "Incomplete"
      | "Downloading"
      | "Extracting"
      | "NeedsLauncher"
      | "Ready"
      | "Error";
    downloaded: boolean;
    extracted: boolean;
    error: string | null;
  };
};
