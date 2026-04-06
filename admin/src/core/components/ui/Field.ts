import { h, createEffect } from 'formajs';

export function Field(props: {
  label: string;
  name: string;
  type?: string;
  value?: () => string;
  onChange?: (value: string) => void;
  placeholder?: string;
  hint?: string;
  required?: boolean;
  readonly?: boolean;
  disabled?: () => boolean;
}) {
  const inputId = 'field-' + props.name;
  const input = props.type === 'select'
    ? h('select', {
        id: inputId,
        name: props.name,
        value: props.value,
        disabled: props.disabled,
        onChange: (e: Event) => props.onChange?.((e.target as HTMLSelectElement).value),
      })
    : props.type === 'textarea'
    ? h('textarea', {
        id: inputId,
        name: props.name,
        value: props.value,
        placeholder: props.placeholder,
        disabled: props.disabled,
        onInput: (e: Event) => props.onChange?.((e.target as HTMLTextAreaElement).value),
      })
    : h('input', {
        id: inputId,
        type: props.type || 'text',
        name: props.name,
        value: props.value,
        placeholder: props.placeholder,
        required: props.required,
        readOnly: props.readonly,
        disabled: props.disabled,
        onInput: (e: Event) => props.onChange?.((e.target as HTMLInputElement).value),
      });

  if (props.value) {
    createEffect(() => {
      const nextValue = props.value ? props.value() : '';
      if (props.type === 'select') {
        const select = input as HTMLSelectElement;
        if (select.value !== nextValue) {
          select.value = nextValue;
        }
        return;
      }

      if (props.type === 'textarea') {
        const textarea = input as HTMLTextAreaElement;
        if (textarea.value !== nextValue) {
          textarea.value = nextValue;
        }
        return;
      }

      const textInput = input as HTMLInputElement;
      if (textInput.value !== nextValue) {
        textInput.value = nextValue;
      }
    });
  }

  return h('div', { class: 'field' },
    h('label', { for: inputId }, props.label),
    input,
    props.hint ? h('div', { class: 'field-hint' }, props.hint) : null,
  );
}
