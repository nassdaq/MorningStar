# TextInput

An editable text. It shows the value it is given and delivers each new value as a message; it has no frame of its own.

{{snippet examples/docs-snippets/src/form.rs#field}}

## Value and messages

{{api crates/morningstar-ui/src/element.rs TextInput only=new,placeholder,placeholder_color,on_input,on_submit}}

## The keyboard

{{api crates/morningstar-ui/src/element.rs TextInput only=keyboard,action,capitalization,no_autocorrect,secure}}

## What the value may hold

{{api crates/morningstar-ui/src/element.rs TextInput only=max_length,digits_only,auto_advance,multiline,max_lines}}

## Style

{{api crates/morningstar-ui/src/element.rs TextInput only=style,role,size,weight,color,caret_color,family,line_height,line_height_dp,letter_spacing,align}}

## The box around it

These are methods of the [`Container`](container.md) that holds the input:

{{api crates/morningstar-ui/src/element.rs Container only=input_frame,focused_border,focused_background}}

The guide page [Text input](../guide/text-input.md) covers selection, the clipboard and what has been verified on a phone.
