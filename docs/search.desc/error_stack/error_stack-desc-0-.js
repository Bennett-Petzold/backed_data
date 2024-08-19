searchState.loadedDescShard("error_stack", 0, "A context-aware error library with arbitrary attached user …\nFrame was created through <code>attach()</code> or <code>attach_printable()</code>.\nClassification of an attachment which is determined by the …\nDefines the current context of a <code>Report</code>.\nFrame was created through <code>Report::new()</code> or <code>change_context()</code>…\nThe <code>Context</code> type of the <code>Result</code>.\nContains the error value\nType of the resulting <code>Err</code> variant wrapped inside a …\nA single context or attachment inside of a <code>Report</code>.\nClassification of the contents of a <code>Frame</code>, determined by …\nExtension trait for <code>Future</code> to provide contextual …\nCompatibility trait to convert from external libraries to …\nContains the success value\nType of the <code>Ok</code> value in the <code>Result</code>\nType of the <code>Ok</code> value in the <code>Result</code>\nA generic attachment created through <code>attach()</code>.\nA printable attachment created through <code>attach_printable()</code>.\nContains a <code>Frame</code> stack consisting of <code>Context</code>s and …\n<code>Result</code><code>&lt;T, </code><code>Report&lt;C&gt;</code><code>&gt;</code>\nExtension trait for <code>Result</code> to provide context information …\nReturns this <code>Report</code> as an <code>Error</code>.\nAdds a new attachment to the <code>Report</code> inside the <code>Result</code> when …\nAdds a new attachment to the <code>Report</code> inside the <code>Result</code>.\nAdds additional information to the <code>Frame</code> stack.\nLazily adds a new attachment to the <code>Report</code> inside the …\nLazily adds a new attachment to the <code>Report</code> inside the …\nAdds a new printable attachment to the <code>Report</code> inside the …\nAdds a new printable attachment to the <code>Report</code> inside the …\nAdds additional (printable) information to the <code>Frame</code> stack.\nLazily adds a new printable attachment to the <code>Report</code> …\nLazily adds a new printable attachment to the <code>Report</code> …\nCreates a <code>Report</code> and returns it as <code>Result</code>.\nCreates a <code>Report</code> and returns it as <code>Result</code>.\nChanges the <code>Context</code> of the <code>Report</code> inside the <code>Result</code> when …\nChanges the context of the <code>Report</code> inside the <code>Result</code>.\nAdd a new <code>Context</code> object to the top of the <code>Frame</code> stack, …\nLazily changes the <code>Context</code> of the <code>Report</code> inside the <code>Result</code> …\nLazily changes the context of the <code>Report</code> inside the <code>Result</code>.\nReturns if <code>T</code> is the type held by any frame inside of the …\nReturns the current context of the <code>Report</code>.\nReturn the direct current frames of this report, to get an …\nDowncasts this frame if the held context or attachment is …\nSearches the frame stack for an instance of type <code>T</code>, …\nDowncasts this frame if the held context or attachment is …\nSearches the frame stack for a context provider <code>T</code> and …\nEnsures <code>$cond</code> is met, otherwise return an error.\nEnsures <code>$cond</code> is met, otherwise return an error.\nMerge two <code>Report</code>s together\nImplementation of formatting, to enable colors and the use …\nReturns an iterator over the <code>Frame</code> stack of the report.\nReturns an iterator over the <code>Frame</code> stack of the report …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nExtension for convenient usage of <code>Report</code>s returned by …\nCan be used to globally set a <code>Debug</code> format hook, for a …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nConverts this <code>Report</code> to an <code>Error</code>.\nConverts the <code>Err</code> variant of the <code>Result</code> to a <code>Report</code>\nReturns if <code>T</code> is the held context or attachment by this …\nIterators over <code>Frame</code>s.\nReturns how the <code>Frame</code> was created.\nCreates a new <code>Report&lt;Context&gt;</code> from a provided scope.\nProvide values which can then be requested by <code>Report</code>.\nProvide values which can then be requested by <code>Report</code>.\nCreates a <code>Report</code> from the given parameters.\nCreates a <code>Report</code> from the given parameters.\nRequests the reference to <code>T</code> from the <code>Frame</code> if provided.\nCreates an iterator of references of type <code>T</code> that have been …\nRequests the value of <code>T</code> from the <code>Frame</code> if provided.\nCreates an iterator of values of type <code>T</code> that have been …\nSet the charset preference\nSet the color mode preference\nReturns a shared reference to the source of this <code>Frame</code>.\nReturns a mutable reference to the sources of this <code>Frame</code>.\nReturns the <code>TypeId</code> of the held context or attachment by …\nTerminal of the user supports ASCII\nThe available supported charsets\nUser preference to enable colors\nThe available modes of color support\nUser preference to enable styles, but discourage colors\nCarrier for contextual information used across hook …\nUser preference to disable all colors\nTerminal of the user supports utf-8\nReturns if the currently requested format should render …\nCast the <code>HookContext</code> to a new type <code>U</code>.\nThe requested <code>Charset</code> for this invocation of hooks\nThe requested <code>ColorMode</code> for this invocation of hooks.\nOne of the most common interactions with <code>HookContext</code> is a …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturn a reference to a value of type <code>U</code>, if a value of …\nReturn a mutable reference to a value of type <code>U</code>, if a …\nOne of the most common interactions with <code>HookContext</code> is a …\nInsert a new value of type <code>U</code> into the storage of …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nThe contents of the appendix are going to be displayed …\nAdd a new entry to the body.\nRemove the value of type <code>U</code> from the storage of <code>HookContext</code> …\nExtension trait for <code>Future</code> to provide contextual …\nAdaptor returned by <code>FutureExt::attach</code>.\nAdaptor returned by <code>FutureExt::change_context</code>.\nAdaptor returned by <code>FutureExt::attach_lazy</code>.\nAdaptor returned by <code>FutureExt::change_context_lazy</code>.\nAdaptor returned by <code>FutureExt::attach_printable_lazy</code>.\nAdaptor returned by <code>FutureExt::attach_printable</code>.\nAdds a new attachment to the <code>Report</code> inside the <code>Result</code> when …\nLazily adds a new attachment to the <code>Report</code> inside the …\nAdds a new printable attachment to the <code>Report</code> inside the …\nLazily adds a new printable attachment to the <code>Report</code> …\nChanges the <code>Context</code> of the <code>Report</code> inside the <code>Result</code> when …\nLazily changes the <code>Context</code> of the <code>Report</code> inside the <code>Result</code> …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nIterator over the <code>Frame</code> stack of a <code>Report</code>.\nIterator over the mutable <code>Frame</code> stack of a <code>Report</code>.\nIterator over requested references in the <code>Frame</code> stack of a …\nIterator over requested values in the <code>Frame</code> stack of a …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.")