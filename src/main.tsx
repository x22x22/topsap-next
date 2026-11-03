import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";

import App from "./App";
import { I18nProvider } from "./lib/i18n";
import { queryClient } from "./lib/query";
import "./index.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("找不到 #root 挂载节点");
}

createRoot(container).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <I18nProvider>
        <App />
      </I18nProvider>
    </QueryClientProvider>
  </StrictMode>,
);
