import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Density = "comfortable" | "compact";
export type Theme = "dark" | "light";

interface UiState {
  density: Density;
  theme: Theme;
  selectedAgentId: string | null;
  setDensity: (density: Density) => void;
  setTheme: (theme: Theme) => void;
  setSelectedAgentId: (id: string | null) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      density: "comfortable",
      theme: "dark",
      selectedAgentId: null,
      setDensity: (density) => set({ density }),
      setTheme: (theme) => set({ theme }),
      setSelectedAgentId: (selectedAgentId) => set({ selectedAgentId })
    }),
    {
      name: "synapse-command-center-ui",
      partialize: (state) => ({
        density: state.density,
        theme: state.theme
      })
    }
  )
);
