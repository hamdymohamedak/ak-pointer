import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("ak", {
  listApps: () => ipcRenderer.invoke("list-apps"),
  catalog: () => ipcRenderer.invoke("catalog"),
  startRecording: (payload: { name: string; apps: Array<{ owner: string; title: string }>; replace: boolean }) =>
    ipcRenderer.invoke("start-recording", payload),
  stop: () => ipcRenderer.invoke("stop"),
  poll: () => ipcRenderer.invoke("poll"),
  play: (names: string[]) => ipcRenderer.invoke("play", names),
  load: (name: string) => ipcRenderer.invoke("load", name),
  save: (name: string, action: unknown) => ipcRenderer.invoke("save", name, action),
  saveRaw: (name: string, text: string) => ipcRenderer.invoke("save-raw", name, text),
  delete: (name: string) => ipcRenderer.invoke("delete", name),
});
