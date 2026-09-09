import React, { useState, useEffect } from 'react';
import { IoIosCloseCircle, IoIosWarning, IoIosInformationCircle } from 'react-icons/io';
import { FaPencilAlt, FaSave } from 'react-icons/fa';
import { cn } from '../../utils';
import { errorMessage } from '../../utils/conversionUtils';
import { Alert, AlertType } from './types';
import { useConfig } from '../ConfigContext';
import { defineMessages, useIntl } from '../../i18n';

const alertIcons: Record<AlertType, React.ReactNode> = {
  [AlertType.Error]: <IoIosCloseCircle className="h-5 w-5" />,
  [AlertType.Warning]: <IoIosWarning className="h-5 w-5" />,
  [AlertType.Info]: <IoIosInformationCircle className="h-5 w-5" />,
};

interface AlertBoxProps {
  alert: Alert;
  className?: string;
  compactButtonEnabled?: boolean;
}

const i18n = defineMessages({
  autoCompactAt: {
    id: 'alertBox.autoCompactAt',
    defaultMessage: 'Auto compact at',
  },
  autoCompactReduceBy: {
    id: 'alertBox.autoCompactReduceBy',
    defaultMessage: 'Reduce by',
  },
  compactNow: {
    id: 'alertBox.compactNow',
    defaultMessage: 'Compact now',
  },
  failedToSaveThreshold: {
    id: 'alertBox.failedToSaveThreshold',
    defaultMessage: 'Failed to save threshold: {error}',
  },
  failedToSaveReduction: {
    id: 'alertBox.failedToSaveReduction',
    defaultMessage: 'Failed to save reduction: {error}',
  },
});

const alertStyles: Record<AlertType, string> = {
  [AlertType.Error]: 'bg-[#d7040e] text-white',
  [AlertType.Warning]: 'bg-[#cc4b03] text-white',
  [AlertType.Info]: 'dark:bg-white dark:text-black bg-black text-white',
};

interface EditablePercentPreferenceProps {
  configKey: string;
  defaultValue: number;
  minPercent: number;
  label: string;
  failedMessage: (error: string) => string;
  onSaved?: (value: number) => void;
}

// Shared by the auto-compact threshold and reduction controls below: both are a
// config value stored as a 0-1 fraction, edited in the UI as a whole percentage,
// with the same load / inline-edit / save interaction.
const EditablePercentPreference = ({
  configKey,
  defaultValue,
  minPercent,
  label,
  failedMessage,
  onSaved,
}: EditablePercentPreferenceProps) => {
  const { read, upsert } = useConfig();
  const [isEditing, setIsEditing] = useState(false);
  const [loadedValue, setLoadedValue] = useState<number>(defaultValue);
  const [percentValue, setPercentValue] = useState(Math.max(minPercent, Math.round(defaultValue * 100)));
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    const loadValue = async () => {
      try {
        const value = await read(configKey, false);
        if (value !== undefined && value !== null && typeof value === 'number') {
          setLoadedValue(value);
          setPercentValue(Math.max(minPercent, Math.round(value * 100)));
        }
      } catch (err) {
        console.error(`Error fetching ${configKey}:`, err);
      }
    };

    loadValue();
    // configKey/minPercent identify a single, unchanging control instance.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [read]);

  const handleSave = async () => {
    if (isSaving) return; // Prevent double-clicks

    const validValue = Math.max(minPercent, Math.min(100, percentValue));
    if (validValue !== percentValue) {
      setPercentValue(validValue);
    }

    setIsSaving(true);
    try {
      const newValue = validValue / 100; // Convert percentage to decimal

      await upsert(configKey, newValue, false);

      setIsEditing(false);
      setLoadedValue(newValue);
      onSaved?.(newValue);
    } catch (error) {
      console.error(`Error saving ${configKey}:`, error);
      window.alert(failedMessage(errorMessage(error, 'Unknown error')));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div
      className="flex items-center justify-center gap-1 min-h-[20px]"
      onMouseDown={(e) => {
        // Prevent a containing popover from closing when clicking inside this control.
        if (isEditing) {
          e.stopPropagation();
        }
      }}
    >
      {isEditing ? (
        <>
          <span className="text-[10px] opacity-70">{label}</span>
          <input
            type="number"
            min={minPercent}
            max="100"
            step="1"
            value={percentValue}
            onChange={(e) => {
              const val = parseInt(e.target.value, 10);
              if (e.target.value === '') {
                setPercentValue(minPercent);
              } else if (!isNaN(val)) {
                setPercentValue(Math.max(minPercent, Math.min(100, val)));
              }
            }}
            onBlur={(e) => {
              const val = parseInt(e.target.value, 10);
              if (isNaN(val) || val < minPercent) {
                setPercentValue(minPercent);
              } else if (val > 100) {
                setPercentValue(100);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                handleSave();
              } else if (e.key === 'Escape') {
                setIsEditing(false);
                setPercentValue(Math.max(minPercent, Math.round(loadedValue * 100)));
              }
            }}
            onFocus={(e) => {
              e.target.select();
            }}
            onClick={(e) => {
              e.stopPropagation();
            }}
            className="w-12 px-1 text-[10px] bg-white/10 border border-current/30 rounded outline-none text-center focus:bg-white/20 focus:border-current/50 transition-colors"
            disabled={isSaving}
            autoFocus
          />
          <span className="text-[10px] opacity-70">%</span>
          <button
            type="button"
            onMouseDown={(e) => {
              e.preventDefault();
              e.stopPropagation();
              handleSave();
            }}
            disabled={isSaving}
            className="p-1 hover:opacity-60 transition-opacity cursor-pointer relative z-50"
            style={{ minWidth: '20px', minHeight: '20px', pointerEvents: 'auto' }}
          >
            <FaSave className="w-3 h-3" />
          </button>
        </>
      ) : (
        <>
          <span className="text-[10px] opacity-70">
            {label} {Math.round(loadedValue * 100)}%
          </span>
          <button
            type="button"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              setIsEditing(true);
            }}
            className="p-1 hover:opacity-60 transition-opacity cursor-pointer relative z-10"
            style={{ minWidth: '20px', minHeight: '20px' }}
          >
            <FaPencilAlt className="w-3 h-3 opacity-70" />
          </button>
        </>
      )}
    </div>
  );
};

export const AlertBox = ({ alert, className }: AlertBoxProps) => {
  const intl = useIntl();

  return (
    <div className={cn('flex flex-col gap-2 px-3 py-3', alertStyles[alert.type], className)}>
      {alert.progress ? (
        <div className="flex flex-col gap-2">
          <EditablePercentPreference
            configKey="GOSLING_AUTO_COMPACT_THRESHOLD"
            defaultValue={0.8}
            minPercent={1}
            label={intl.formatMessage(i18n.autoCompactAt)}
            failedMessage={(error) => intl.formatMessage(i18n.failedToSaveThreshold, { error })}
            onSaved={alert.onThresholdChange}
          />
          <EditablePercentPreference
            configKey="GOSLING_AUTO_COMPACT_REDUCTION"
            defaultValue={0.15}
            minPercent={0}
            label={intl.formatMessage(i18n.autoCompactReduceBy)}
            failedMessage={(error) => intl.formatMessage(i18n.failedToSaveReduction, { error })}
          />
          {alert.showCompactButton && alert.onCompact && (
            <button
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                alert.onCompact!();
              }}
              disabled={alert.compactButtonDisabled}
              className={cn(
                'flex items-center justify-center gap-1.5 text-[11px] outline-none',
                alert.compactButtonDisabled
                  ? 'opacity-50 cursor-not-allowed'
                  : 'hover:opacity-80 cursor-pointer'
              )}
            >
              {alert.compactIcon}
              <span>{intl.formatMessage(i18n.compactNow)}</span>
            </button>
          )}
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <div className="flex-shrink-0">{alertIcons[alert.type]}</div>
            <div className="flex flex-col gap-2 flex-1">
              <span className="text-[11px] break-words whitespace-pre-line">{alert.message}</span>
              {alert.action && (
                <a
                  role="button"
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    alert.action?.onClick();
                  }}
                  className="text-[11px] text-left underline hover:opacity-80 cursor-pointer outline-none"
                >
                  {alert.action.text}
                </a>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
};
