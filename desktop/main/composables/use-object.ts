import { convertFileSrc } from "@tauri-apps/api/core";

export const useObject = (id: string) => {
  if (!id)
    return `${useRuntimeConfig().app.baseURL.replace(/\/?$/, "/")}local-game.svg`;
  return convertFileSrc(id, "object");
};
