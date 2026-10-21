import { Button } from "@/components/ui/button"
import { useTranslation } from "react-i18next"

interface PageErrorProps {
  message: string
  onRetry?: () => void
}

/** Inline page-level failure state with a retry action (replaces console-only
 * error handling so users can see and recover from failed loads). */
export function PageError({ message, onRetry }: PageErrorProps) {
  const { t } = useTranslation()
  return (
    <div className="h-full w-full flex-1 flex flex-col items-center justify-center gap-3 px-6 text-center">
      <p className="max-w-[440px] text-sm text-muted-foreground break-words">{message}</p>
      {onRetry && (
        <Button variant="outline" size="sm" onClick={onRetry}>
          {t("common.retry")}
        </Button>
      )}
    </div>
  )
}
