<script lang="ts">
    /**
     * Svelte sample file for tree-sitter syntax highlighting test
     * Phase 1 language support
     */

    import { onMount, onDestroy, createEventDispatcher } from 'svelte';
    import { writable, derived, type Writable } from 'svelte/store';

    // Props with TypeScript types
    export let title: string = 'Tree-sitter Highlighter';
    export let languages: string[] = ['lua', 'bash', 'php', 'swift', 'haskell', 'svelte'];
    export let theme: 'light' | 'dark' = 'dark';

    // Event dispatcher
    const dispatch = createEventDispatcher<{
        select: { language: string };
        highlight: { code: string; tokens: Token[] };
    }>();

    // Type definitions
    interface Token {
        start: number;
        end: number;
        scope: string;
        text: string;
    }

    interface HighlightResult {
        language: string;
        tokens: Token[];
        duration: number;
    }

    // Reactive stores
    const selectedLanguage: Writable<string | null> = writable(null);
    const highlightResults = writable<HighlightResult[]>([]);

    // Derived store
    const tokenCount = derived(highlightResults, ($results) =>
        $results.reduce((sum, r) => sum + r.tokens.length, 0)
    );

    // Local state
    let codeInput = '';
    let isProcessing = false;
    let error: string | null = null;

    // Reactive state (declare with let before reactive assignment)
    let hasCode = false;
    let canHighlight = false;
    let themeClass = '';

    // Reactive statements
    $: hasCode = codeInput.trim().length > 0;
    $: canHighlight = hasCode && $selectedLanguage !== null && !isProcessing;
    $: themeClass = theme === 'dark' ? 'bg-gray-900 text-white' : 'bg-white text-gray-900';

    // Lifecycle
    onMount(() => {
        console.log('Highlighter component mounted');
        loadSavedState();
    });

    onDestroy(() => {
        console.log('Highlighter component destroyed');
    });

    // Functions
    function loadSavedState(): void {
        const saved = localStorage.getItem('highlighter-state');
        if (saved) {
            try {
                const state = JSON.parse(saved);
                selectedLanguage.set(state.language);
                codeInput = state.code || '';
            } catch (e) {
                console.error('Failed to load saved state:', e);
            }
        }
    }

    async function handleHighlight(): Promise<void> {
        if (!canHighlight) return;

        isProcessing = true;
        error = null;

        try {
            const startTime = performance.now();
            const tokens = await simulateHighlight(codeInput, $selectedLanguage!);
            const duration = performance.now() - startTime;

            const result: HighlightResult = {
                language: $selectedLanguage!,
                tokens,
                duration,
            };

            highlightResults.update((results) => [...results, result]);
            dispatch('highlight', { code: codeInput, tokens });
        } catch (e) {
            error = e instanceof Error ? e.message : 'Unknown error';
        } finally {
            isProcessing = false;
        }
    }

    async function simulateHighlight(code: string, language: string): Promise<Token[]> {
        // Simulate async processing
        await new Promise((resolve) => setTimeout(resolve, 100));

        const tokens: Token[] = [];
        const keywords = getKeywordsForLanguage(language);

        keywords.forEach((keyword) => {
            let index = 0;
            while ((index = code.indexOf(keyword, index)) !== -1) {
                tokens.push({
                    start: index,
                    end: index + keyword.length,
                    scope: 'keyword',
                    text: keyword,
                });
                index += keyword.length;
            }
        });

        return tokens;
    }

    function getKeywordsForLanguage(lang: string): string[] {
        const keywordMap: Record<string, string[]> = {
            lua: ['function', 'local', 'if', 'then', 'else', 'end', 'for', 'while', 'return'],
            bash: ['if', 'then', 'else', 'fi', 'for', 'while', 'do', 'done', 'function'],
            php: ['function', 'class', 'public', 'private', 'if', 'else', 'foreach', 'return'],
            swift: ['func', 'class', 'struct', 'enum', 'let', 'var', 'if', 'else', 'return'],
            haskell: ['module', 'where', 'import', 'data', 'type', 'class', 'instance', 'let', 'in'],
            svelte: ['export', 'let', 'const', 'function', 'if', 'else', 'each', 'await'],
        };
        return keywordMap[lang] || [];
    }

    function selectLanguage(lang: string): void {
        selectedLanguage.set(lang);
        dispatch('select', { language: lang });
    }

    function clearResults(): void {
        highlightResults.set([]);
        error = null;
    }
</script>

<div class="highlighter-container {themeClass}">
    <header class="header">
        <h1>{title}</h1>
        <p>Total tokens: {$tokenCount}</p>
    </header>

    <nav class="language-selector">
        {#each languages as lang (lang)}
            <button
                class="lang-btn"
                class:selected={$selectedLanguage === lang}
                on:click={() => selectLanguage(lang)}
                disabled={isProcessing}
            >
                {lang}
            </button>
        {/each}
    </nav>

    <main class="content">
        <section class="input-section">
            <label for="code-input">Enter code to highlight:</label>
            <textarea
                id="code-input"
                bind:value={codeInput}
                placeholder="Paste your code here..."
                rows="10"
                disabled={isProcessing}
            />
        </section>

        <div class="actions">
            <button
                class="highlight-btn"
                on:click={handleHighlight}
                disabled={!canHighlight}
            >
                {#if isProcessing}
                    Processing...
                {:else}
                    Highlight Code
                {/if}
            </button>
            <button class="clear-btn" on:click={clearResults}>
                Clear Results
            </button>
        </div>

        {#if error}
            <div class="error" role="alert">
                <strong>Error:</strong> {error}
            </div>
        {/if}

        <section class="results">
            <h2>Results</h2>
            {#if $highlightResults.length === 0}
                <p class="no-results">No highlighting results yet.</p>
            {:else}
                <ul class="result-list">
                    {#each $highlightResults as result, index (index)}
                        <li class="result-item">
                            <span class="result-lang">{result.language}</span>
                            <span class="result-tokens">{result.tokens.length} tokens</span>
                            <span class="result-time">{result.duration.toFixed(2)}ms</span>
                        </li>
                    {/each}
                </ul>
            {/if}
        </section>

        {#await Promise.resolve($highlightResults)}
            <p>Loading...</p>
        {:then results}
            {#if results.length > 0}
                <details>
                    <summary>Token Details</summary>
                    <pre>{JSON.stringify(results, null, 2)}</pre>
                </details>
            {/if}
        {:catch error}
            <p>Error loading results: {error.message}</p>
        {/await}
    </main>
</div>

<style>
    .highlighter-container {
        max-width: 800px;
        margin: 0 auto;
        padding: 2rem;
        font-family: system-ui, -apple-system, sans-serif;
    }

    .header {
        text-align: center;
        margin-bottom: 2rem;
    }

    .header h1 {
        font-size: 2rem;
        margin-bottom: 0.5rem;
    }

    .language-selector {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        justify-content: center;
        margin-bottom: 1.5rem;
    }

    .lang-btn {
        padding: 0.5rem 1rem;
        border: 2px solid currentColor;
        border-radius: 0.25rem;
        background: transparent;
        cursor: pointer;
        transition: all 0.2s;
    }

    .lang-btn:hover:not(:disabled) {
        background: rgba(128, 128, 128, 0.2);
    }

    .lang-btn.selected {
        background: #50fa7b;
        color: #282a36;
        border-color: #50fa7b;
    }

    .lang-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    textarea {
        width: 100%;
        padding: 1rem;
        font-family: 'Fira Code', monospace;
        font-size: 0.9rem;
        border: 1px solid #ccc;
        border-radius: 0.25rem;
        resize: vertical;
    }

    .actions {
        display: flex;
        gap: 1rem;
        margin: 1rem 0;
    }

    .highlight-btn,
    .clear-btn {
        padding: 0.75rem 1.5rem;
        border: none;
        border-radius: 0.25rem;
        cursor: pointer;
        font-weight: bold;
    }

    .highlight-btn {
        background: #50fa7b;
        color: #282a36;
    }

    .highlight-btn:disabled {
        background: #6272a4;
        cursor: not-allowed;
    }

    .clear-btn {
        background: #ff5555;
        color: white;
    }

    .error {
        padding: 1rem;
        background: #ff5555;
        color: white;
        border-radius: 0.25rem;
        margin: 1rem 0;
    }

    .result-list {
        list-style: none;
        padding: 0;
    }

    .result-item {
        display: flex;
        gap: 1rem;
        padding: 0.5rem;
        border-bottom: 1px solid rgba(128, 128, 128, 0.3);
    }

    .result-lang {
        font-weight: bold;
        color: #50fa7b;
    }

    .result-tokens {
        color: #8be9fd;
    }

    .result-time {
        color: #6272a4;
        margin-left: auto;
    }

    details {
        margin-top: 1rem;
    }

    pre {
        background: rgba(0, 0, 0, 0.2);
        padding: 1rem;
        border-radius: 0.25rem;
        overflow-x: auto;
        font-size: 0.8rem;
    }
</style>
