import { DocumentUri, TextDocuments } from 'vscode-languageserver';
import {
	TextDocument
} from 'vscode-languageserver-textdocument';

// eslint-disable-next-line @typescript-eslint/no-var-requires
const Parser = require('circom/parser/jaz.js');

export class LspDatabase {
	documents: TextDocuments<TextDocument>; 
	parseTrees: Map<DocumentUri, any>;
	constructor() {
		this.documents = new TextDocuments(TextDocument);
		this.parseTrees = new Map();
	}

	connectToLSP() {
		console.log("hello");
	}

	updateParseTree(uri: DocumentUri, text: string) {
		try {
			const tree = Parser.parse(text);
			this.parseTrees.set(uri, tree);
		// eslint-disable-next-line no-empty
		} catch(err) {
		}
	}

	getLatestParseTree(documentUri: DocumentUri): any{
		return this.parseTrees.get(documentUri);
	}
}