import MDEditor from '@uiw/react-md-editor/nohighlight';
import rehypeSanitize from 'rehype-sanitize';
import '@uiw/react-md-editor/markdown-editor.css';
import '@uiw/react-markdown-preview/markdown.css';

type MarkdownDocumentProps = {
  value: string;
  editing?: boolean;
  onChange?: (value: string) => void;
};

export default function MarkdownDocument({
  value,
  editing = false,
  onChange,
}: MarkdownDocumentProps) {
  if (!editing) {
    return (
      <article className="markdown-preview" data-color-mode="light">
        <MDEditor.Markdown source={value} rehypePlugins={[[rehypeSanitize]]} />
      </article>
    );
  }

  return (
    <div className="markdown-editor-shell">
      <MDEditor
        className="markdown-md-editor"
        data-color-mode="light"
        height={520}
        preview="live"
        value={value}
        onChange={(nextValue) => onChange?.(nextValue ?? '')}
        textareaProps={{ spellCheck: false }}
        previewOptions={{ rehypePlugins: [[rehypeSanitize]] }}
      />
    </div>
  );
}
