import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../i18n/test-utils';
import { useNavigationContext } from './NavigationContext';
import { AppEvents } from '../../constants/events';
import { useNavigationSessions } from '../../hooks/useNavigationSessions';
import { Navigation } from './NavigationPanel';

vi.mock('./NavigationContext', () => ({
  useNavigationContext: vi.fn(),
}));

vi.mock('../../hooks/useNavigationSessions', () => ({
  useNavigationSessions: vi.fn(),
}));

vi.mock('react-router-dom', () => ({
  useLocation: () => ({ pathname: '/' }),
  useNavigate: () => vi.fn(),
}));

vi.mock('../workspaces/WorkspaceSidebarSection', () => ({
  WorkspaceSidebarSection: ({ readyWorkspaceIds }: { readyWorkspaceIds: ReadonlySet<string> }) => (
    <div data-testid="workspaces">
      Workspaces
      {[...readyWorkspaceIds].map((id) => (
        <span key={id}>Ready workspace: {id}</span>
      ))}
    </div>
  ),
}));

const WORKSPACES_HEIGHT_KEY = 'workspaces_sidebar_height';

type ElementRect = ReturnType<HTMLElement['getBoundingClientRect']>;

// `fireEvent` needs a constructed event and the lint config's global allowlist
// has no PointerEvent; the handlers only read `clientY`, which a MouseEvent
// carries, and the listeners key off the event name.
const pointerEvent = (name: string, clientY?: number) =>
  new window.MouseEvent(name, clientY === undefined ? undefined : { clientY });

// The panel is laid out by flexbox, which jsdom does not run, so the divider's
// clamp is exercised through stubbed geometry: the pane starts at y=100 and the
// panel ends at y=700, leaving 480px of travel above the chats minimum.
function stubGeometry(paneHeight: number) {
  const pane = screen.getByTestId('workspaces').parentElement as HTMLElement;
  const panel = pane.parentElement as HTMLElement;
  vi.spyOn(pane, 'getBoundingClientRect').mockReturnValue({
    top: 100,
    bottom: 100 + paneHeight,
    height: paneHeight,
  } as ElementRect);
  vi.spyOn(panel, 'getBoundingClientRect').mockReturnValue({
    top: 0,
    bottom: 700,
    height: 700,
  } as ElementRect);
  return pane;
}

describe('NavigationPanel workspaces/chats divider', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    vi.mocked(useNavigationContext).mockReturnValue({
      isNavExpanded: true,
    } as ReturnType<typeof useNavigationContext>);
    vi.mocked(useNavigationSessions).mockReturnValue({
      recentSessions: [],
      activeSessionId: null,
      fetchSessions: vi.fn(),
      handleNavClick: vi.fn(),
      handleSessionClick: vi.fn(),
    } as unknown as ReturnType<typeof useNavigationSessions>);
  });

  const renderPanel = () =>
    render(
      <IntlTestWrapper>
        <Navigation />
      </IntlTestWrapper>
    );

  it('sizes the workspaces list with its content until the divider is dragged', () => {
    renderPanel();

    const pane = screen.getByTestId('workspaces').parentElement as HTMLElement;
    expect(pane.className).toContain('max-h-[45%]');
    expect(pane.style.height).toBe('');
  });

  it('drags the divider to a new split and remembers it', () => {
    renderPanel();
    const pane = stubGeometry(200);
    const divider = screen.getByRole('separator');

    fireEvent.pointerDown(divider, { clientY: 300 });
    fireEvent(window, pointerEvent('pointermove', 380));
    fireEvent(window, pointerEvent('pointerup'));

    expect(pane.style.height).toBe('280px');
    expect(pane.className).not.toContain('max-h-[45%]');
    expect(window.localStorage.getItem(WORKSPACES_HEIGHT_KEY)).toBe('280');
  });

  it('will not drag a section out of existence', () => {
    renderPanel();
    const pane = stubGeometry(200);
    const divider = screen.getByRole('separator');

    // Far past the top: the workspaces list keeps its minimum.
    fireEvent.pointerDown(divider, { clientY: 300 });
    fireEvent(window, pointerEvent('pointermove', -900));
    fireEvent(window, pointerEvent('pointerup'));
    expect(pane.style.height).toBe('72px');

    // Far past the bottom: the chats list keeps its minimum (700 - 100 - 120).
    fireEvent.pointerDown(divider, { clientY: 300 });
    fireEvent(window, pointerEvent('pointermove', 9000));
    fireEvent(window, pointerEvent('pointerup'));
    expect(pane.style.height).toBe('480px');
  });

  it('moves the divider with the arrow keys', () => {
    renderPanel();
    const pane = stubGeometry(200);
    const divider = screen.getByRole('separator');

    fireEvent.keyDown(divider, { key: 'ArrowDown' });
    expect(pane.style.height).toBe('224px');
    expect(window.localStorage.getItem(WORKSPACES_HEIGHT_KEY)).toBe('224');
  });

  it('restores content sizing on a double-click', () => {
    window.localStorage.setItem(WORKSPACES_HEIGHT_KEY, '300');
    renderPanel();

    const pane = screen.getByTestId('workspaces').parentElement as HTMLElement;
    expect(pane.style.height).toBe('300px');

    fireEvent.doubleClick(screen.getByRole('separator'));

    expect(pane.style.height).toBe('');
    expect(pane.className).toContain('max-h-[45%]');
    expect(window.localStorage.getItem(WORKSPACES_HEIGHT_KEY)).toBeNull();
  });
});

describe('NavigationPanel workspace readiness', () => {
  const onSessionClick = vi.fn();
  const firstSession = {
    id: 'math-1',
    workspaceId: 'math',
    name: 'First math chat',
    workingDir: '/math',
    createdAt: '2026-09-08T10:00:00Z',
    updatedAt: '',
    messageCount: 2,
  };
  const secondSession = { ...firstSession, id: 'math-2', name: 'Second math chat' };

  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    vi.mocked(useNavigationContext).mockReturnValue({
      isNavExpanded: true,
      setIsNavExpanded: vi.fn(),
    });
    vi.mocked(useNavigationSessions).mockReturnValue({
      recentSessions: [],
      activeSessionId: undefined,
      fetchSessions: vi.fn(),
      handleNavClick: vi.fn(),
      handleSessionClick: onSessionClick,
    });
  });

  const renderPanel = () => render(<Navigation />, { wrapper: IntlTestWrapper });
  const status = (sessionId: string, workspaceId: string | null, streamState: string) =>
    fireEvent(
      window,
      new CustomEvent(AppEvents.SESSION_STATUS_UPDATE, {
        detail: { sessionId, workspaceId, streamState },
      })
    );
  const finishChat = (sessionId: string, workspaceId: string | null) => {
    status(sessionId, workspaceId, 'streaming');
    status(sessionId, workspaceId, 'idle');
  };

  it('keeps a completed chat workspace ready outside the filtered recent list and collapsed chats', () => {
    renderPanel();
    status('math-1', 'math', 'idle');
    expect(screen.queryByText('Ready workspace: math')).not.toBeInTheDocument();
    status('math-1', 'math', 'streaming');
    expect(screen.queryByText('Ready workspace: math')).not.toBeInTheDocument();
    status('math-1', 'math', 'idle');
    expect(screen.getByText('Ready workspace: math')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Chats' }));
    expect(screen.getByText('Ready workspace: math')).toBeInTheDocument();
  });

  it('clears the workspace only after its last ready chat is opened and preserves the chat dots', () => {
    const context = vi.mocked(useNavigationSessions)();
    vi.mocked(useNavigationSessions).mockReturnValue({
      ...context,
      recentSessions: [firstSession, secondSession],
    });
    const { rerender } = renderPanel();
    finishChat('math-1', 'math');
    finishChat('math-2', 'math');
    expect(screen.getAllByLabelText('Has new activity')).toHaveLength(2);

    vi.mocked(useNavigationSessions).mockReturnValue({ ...context, recentSessions: [] });
    rerender(<Navigation />);
    expect(screen.getByText('Ready workspace: math')).toBeInTheDocument();
    vi.mocked(useNavigationSessions).mockReturnValue({
      ...context,
      recentSessions: [firstSession, secondSession],
    });
    rerender(<Navigation />);

    fireEvent.click(screen.getByText('First math chat'));
    expect(onSessionClick).toHaveBeenCalledWith('math-1');
    expect(screen.getAllByLabelText('Has new activity')).toHaveLength(1);
    expect(screen.getByText('Ready workspace: math')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Second math chat'));
    expect(screen.queryByLabelText('Has new activity')).not.toBeInTheDocument();
    expect(screen.queryByText('Ready workspace: math')).not.toBeInTheDocument();
  });

  it('excludes error and streaming chats but keeps other ready chats in that workspace', () => {
    renderPanel();
    finishChat('math-1', 'math');
    status('math-1', 'math', 'streaming');
    expect(screen.queryByText('Ready workspace: math')).not.toBeInTheDocument();
    status('math-1', 'math', 'error');
    expect(screen.queryByText('Ready workspace: math')).not.toBeInTheDocument();
    finishChat('math-2', 'math');
    expect(screen.getByText('Ready workspace: math')).toBeInTheDocument();
    finishChat('physics-1', 'physics');
    expect(screen.getByText('Ready workspace: physics')).toBeInTheDocument();
  });

  it.each([AppEvents.SESSION_ARCHIVED, AppEvents.SESSION_DELETED])(
    'removes readiness when the chat is %s',
    (eventName) => {
      renderPanel();
      finishChat('math-1', 'math');
      finishChat('math-2', 'math');
      fireEvent(window, new CustomEvent(eventName, { detail: { sessionId: 'math-1' } }));
      expect(screen.getByText('Ready workspace: math')).toBeInTheDocument();
      fireEvent(window, new CustomEvent(eventName, { detail: { sessionId: 'math-2' } }));
      expect(screen.queryByText('Ready workspace: math')).not.toBeInTheDocument();
    }
  );

  it('does not assign an unassigned chat to a workspace, and updates when its metadata arrives', () => {
    renderPanel();
    finishChat('math-1', null);
    expect(screen.getByTestId('workspaces')).toHaveTextContent(/^Workspaces$/);
    status('math-1', 'math', 'idle');
    expect(screen.getByText('Ready workspace: math')).toBeInTheDocument();
  });
});
