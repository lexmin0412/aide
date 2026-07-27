import { Component } from "react"

interface Props {
  children: React.ReactNode
}

interface State {
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  render() {
    if (this.state.error) {
      return (
        <div className="h-full flex flex-col items-center justify-center p-8 text-center">
          <p className="text-sm font-semibold text-destructive mb-2">Something went wrong</p>
          <pre className="text-xs text-muted-foreground max-w-md overflow-auto whitespace-pre-wrap font-mono">
            {this.state.error.message}
          </pre>
          <button
            onClick={() => this.setState({ error: null })}
            className="mt-4 text-xs text-muted-foreground hover:text-foreground underline"
          >
            Try again
          </button>
        </div>
      )
    }
    return this.props.children
  }
}
