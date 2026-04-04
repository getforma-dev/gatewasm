import { h } from 'formajs';

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
}) {
  const inputId = 'field-' + props.name;
  const input = props.type === 'select'
    ? h('select', {
        id: inputId,
        name: props.name,
        value: props.value,
        onChange: (e: Event) => props.onChange?.((e.target as HTMLSelectElement).value),
      })
    : props.type === 'textarea'
    ? h('textarea', {
        id: inputId,
        name: props.name,
        value: props.value,
        placeholder: props.placeholder,
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
        onInput: (e: Event) => props.onChange?.((e.target as HTMLInputElement).value),
      });

  return h('div', { class: 'field' },
    h('label', { for: inputId }, props.label),
    input,
    props.hint ? h('div', { class: 'field-hint' }, props.hint) : null,
  );
}
