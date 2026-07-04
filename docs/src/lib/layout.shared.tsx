import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";
import { Logo } from "@/components/logo";
import { appName, gitConfig, tagline } from "./shared";

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: (
        <div className="flex items-center gap-2.5">
          <Logo className="h-6 w-6 text-fd-primary" />
          <div className="flex flex-col leading-none">
            <span className="font-bold text-base">{appName}</span>
            <span className="text-fd-muted-foreground hidden text-xs sm:inline">
              {tagline}
            </span>
          </div>
        </div>
      ),
    },
    links: [
      {
        text: "Docs",
        url: "/docs",
        active: "nested-url",
      },
    ],
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
