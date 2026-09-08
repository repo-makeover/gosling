import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../i18n/test-utils';
import { Z_INDEX } from '../Layout/constants';
import { ConfirmationModal } from './ConfirmationModal';
import { Dialog, DialogContent, DialogDescription, DialogTitle, DialogTrigger } from './dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './dropdown-menu';

const props = {
  isOpen: true,
  title: 'Delete Session',
  message: 'Delete this session permanently?',
  confirmLabel: 'Delete Session',
  cancelLabel: 'Cancel',
};

describe('confirmation dialog layering', () => {
  it('portals the dialog and backdrop above the app panes even from a clipped parent', () => {
    const { container } = render(
      <div style={{ transform: 'translateZ(0)', overflow: 'hidden', width: 200 }}>
        <ConfirmationModal {...props} onConfirm={vi.fn()} onCancel={vi.fn()} />
      </div>,
      { wrapper: IntlTestWrapper }
    );
    const dialog = screen.getByRole('dialog', { name: props.title });
    const backdrop = document.querySelector<HTMLElement>('[data-slot="dialog-overlay"]')!;
    expect(container).not.toContainElement(dialog);
    expect(container).not.toContainElement(backdrop);
    expect(Number(backdrop.style.zIndex)).toBeGreaterThan(Z_INDEX.HEADER);
    expect(Number(dialog.style.zIndex)).toBeGreaterThanOrEqual(Number(backdrop.style.zIndex));
    expect(dialog).toHaveAccessibleDescription(props.message);
  });

  it('retains confirm, cancel, and Escape actions', async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmationModal {...props} onConfirm={onConfirm} onCancel={onCancel} />, {
      wrapper: IntlTestWrapper,
    });
    await user.click(screen.getByRole('button', { name: props.confirmLabel }));
    expect(onConfirm).toHaveBeenCalledOnce();
    await user.click(screen.getByRole('button', { name: props.cancelLabel }));
    expect(onCancel).toHaveBeenCalledOnce();
    await user.keyboard('{Escape}');
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it('keeps confirmation buttons disabled while submitting', () => {
    const onConfirm = vi.fn();
    render(<ConfirmationModal {...props} isSubmitting onConfirm={onConfirm} onCancel={vi.fn()} />, {
      wrapper: IntlTestWrapper,
    });
    expect(screen.getByRole('button', { name: 'Processing...' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Processing...' }));
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it('keeps a dialog dropdown above its modal and restores focus after closing', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <Dialog>
        <DialogTrigger>Open settings</DialogTrigger>
        <DialogContent>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Choose an option</DialogDescription>
          <DropdownMenu>
            <DropdownMenuTrigger>Options</DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuItem onSelect={onSelect}>Choose</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </DialogContent>
      </Dialog>,
      { wrapper: IntlTestWrapper }
    );
    const trigger = screen.getByRole('button', { name: 'Open settings' });
    await user.click(trigger);
    const dialog = screen.getByRole('dialog');
    await user.click(screen.getByRole('button', { name: 'Options' }));
    const menu = screen.getByRole('menu');
    expect(Number(menu.style.zIndex)).toBeGreaterThan(Number(dialog.style.zIndex));
    await user.click(screen.getByRole('menuitem', { name: 'Choose' }));
    expect(onSelect).toHaveBeenCalledOnce();
    expect(dialog).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});
