import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TooltipProvider } from "@/components/ui/tooltip";
import { App } from "@/app";
import "@/styles.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchInterval: 3000,
      refetchOnWindowFocus: true,
      staleTime: 2500,
      retry: 1
    }
  }
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <App />
      </TooltipProvider>
    </QueryClientProvider>
  </StrictMode>
);
