import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../../i18n/test-utils';
import { defaultOutputFileExtensions } from '../../../utils/settings';
import OutputFileExtensionsSection from './OutputFileExtensionsSection';

describe('OutputFileExtensionsSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(window.electron.getSetting).mockResolvedValue([...defaultOutputFileExtensions]);
    vi.mocked(window.electron.setSetting).mockResolvedValue();
  });

  it('shows the defaults and adds normalized, deduplicated extensions', async () => {
    const changed = vi.fn();
    window.addEventListener('outputFileExtensionsChanged', changed);

    render(
      <IntlTestWrapper>
        <OutputFileExtensionsSection />
      </IntlTestWrapper>
    );

    expect(await screen.findByText('.docx')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Add file extensions'), {
      target: { value: '.XML .md' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() =>
      expect(window.electron.setSetting).toHaveBeenCalledWith('outputFileExtensions', [
        ...defaultOutputFileExtensions,
        'xml',
      ])
    );
    expect(screen.getByText('.xml')).toBeInTheDocument();
    expect(changed).toHaveBeenCalledTimes(1);

    window.removeEventListener('outputFileExtensionsChanged', changed);
  });

  it('removes extensions and can restore the defaults', async () => {
    render(
      <IntlTestWrapper>
        <OutputFileExtensionsSection />
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Remove .pdf' }));
    await waitFor(() =>
      expect(window.electron.setSetting).toHaveBeenCalledWith(
        'outputFileExtensions',
        defaultOutputFileExtensions.filter((extension) => extension !== 'pdf')
      )
    );

    fireEvent.click(screen.getByRole('button', { name: 'Reset defaults' }));
    await waitFor(() =>
      expect(window.electron.setSetting).toHaveBeenLastCalledWith(
        'outputFileExtensions',
        defaultOutputFileExtensions
      )
    );
  });
});
