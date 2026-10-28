import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DocumentView, EditRequest, Hotkeys, SessionSnapshot, WorkspaceView } from "./types";
const identity = (doc: DocumentView) => ({ taskId: doc.taskId, revision: doc.revision });
export const fetchWorkspace = () => invoke<WorkspaceView>("get_workspace");
export const fetchStatus = () => invoke<SessionSnapshot>("session_status");
export const startRecording = (doc: DocumentView) => invoke<SessionSnapshot>("start_recording", identity(doc));
export const stopRecording = (gesture: string | null = null) => invoke<WorkspaceView>("stop_recording", { gesture });
export const cancelRecording = (doc: DocumentView) => invoke<WorkspaceView>("cancel_recording", identity(doc));
export const retryRecording = (discard = false) => invoke<WorkspaceView>("retry_recording", { discard });
export const createTask = (name: string) => invoke<WorkspaceView>("create_task", { name });
export const selectTask = (taskId: string) => invoke<WorkspaceView>("select_task", { taskId });
export const renameTask = (doc: DocumentView, name: string) =>
    invoke<WorkspaceView>("rename_task", { ...identity(doc), name });
export const deleteTask = (doc: DocumentView) => invoke<WorkspaceView>("delete_task", identity(doc));
export const editTask = (doc: DocumentView, edit: EditRequest) =>
    invoke<WorkspaceView>("edit_task", { ...identity(doc), edit });
export const taskHistory = (doc: DocumentView, redo = false) =>
    invoke<WorkspaceView>("task_history", { ...identity(doc), redo });
export const setHotkeys = (hotkeys: Hotkeys) => invoke<WorkspaceView>("set_hotkeys", { hotkeys });
export const demoMode = (enter: boolean) => invoke<WorkspaceView>("demo_mode", { enter });
export const startExecution = (doc: DocumentView, loopCount: number, speed: number) =>
    invoke<SessionSnapshot>("start_execution", { ...identity(doc), loopCount, speed });
export const stopExecution = () => invoke<SessionSnapshot>("stop_execution");
export const clearError = () => invoke<void>("clear_error");
export const onStateChanged = (callback: (state: SessionSnapshot) => void) =>
    listen<SessionSnapshot>("state-changed", e => callback(e.payload));
